import SwiftUI
import VelaAvatar
import VelaIPC

struct DiagnosticsView: View {
    let environment: AppEnvironment
    @ObservedObject private var client: IPCClient
    @ObservedObject private var supervisor: CoreProcessSupervisor
    @ObservedObject private var avatar: AvatarController
    @State private var selectedAgentID = ""
    @State private var sessionCWD = FileManager.default.currentDirectoryPath
    @State private var promptText = "Summarize this workspace briefly."
    @State private var workspaceRoot = FileManager.default.currentDirectoryPath
    @State private var workspaceStatus = ""
    @State private var referencePath = ""

    init(environment: AppEnvironment) {
        self.environment = environment
        client = environment.client
        supervisor = environment.supervisor
        avatar = environment.avatar
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                VStack(alignment: .leading) {
                    Text("Vela IPC Diagnostics")
                        .font(.title2.bold())
                    Text("App \(appVersion) · IPC 1.0")
                        .foregroundStyle(.secondary)
                }
                Spacer()
                statusBadge
            }

            GroupBox("Runtime") {
                Grid(alignment: .leading, horizontalSpacing: 12, verticalSpacing: 6) {
                    GridRow { Text("Core"); Text(supervisor.state.rawValue) }
                    GridRow { Text("IPC"); Text(client.state.rawValue) }
                    GridRow { Text("Socket"); Text(supervisor.socketPath).textSelection(.enabled) }
                    GridRow {
                        Text("Executable")
                        Text(supervisor.executablePath ?? "Not resolved").textSelection(.enabled)
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }

            HStack {
                Button("Start / Restart Core") { Task { await environment.restart() } }
                Button("Health") { client.requestHealth() }
                    .disabled(client.state != .ready)
                Button("Quick Capture") { environment.showQuickCapture() }
                    .disabled(client.workspace == nil)
                Button("Start 20-event Stream") { client.startStream() }
                    .disabled(client.state != .ready || client.activeStreamRequestID != nil)
                Button("Cancel Stream") { client.cancelStream() }
                    .disabled(client.activeStreamRequestID == nil)
                Button("Kill Core") { supervisor.killForDiagnostics() }
                    .disabled(supervisor.state != .running)
            }

            workspacePanel
            capturePanel
            currentStatePanel
            avatarPanel
            agentList
            sessionPanel
            permissionPanel

            HSplitView {
                eventList
                diagnosticList
            }
        }
        .padding(20)
        .frame(minWidth: 900, minHeight: 1600)
        .onChange(of: client.agentRegistry.agents) { _, agents in
            if !agents.contains(where: { $0.id == selectedAgentID && $0.status == .ready }) {
                selectedAgentID = agents.first(where: { $0.status == .ready })?.id ?? ""
            }
        }
        .onChange(of: client.workspace?.statusMarkdown) { _, status in
            if let status {
                workspaceStatus = status
            }
        }
    }

    private var statusBadge: some View {
        Text(client.state == .ready ? "Ready" : "Degraded")
            .font(.headline)
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(client.state == .ready ? Color.green.opacity(0.2) : Color.orange.opacity(0.2))
            .clipShape(Capsule())
    }

    private var eventList: some View {
        GroupBox {
            List(client.transcript.events) { event in
                VStack(alignment: .leading, spacing: 3) {
                    Text(event.name).font(.headline)
                    if let sequence = event.sequence, let text = event.text {
                        Text("\(sequence): \(text)")
                    }
                    Text(event.requestID).font(.caption).foregroundStyle(.secondary)
                }
            }
        } label: {
            HStack {
                Text("Stream Events")
                Spacer()
                Button("Clear") { client.clearEvents() }.buttonStyle(.link)
            }
        }
    }

    private var workspacePanel: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    TextField("Workspace root", text: $workspaceRoot)
                    Button("Open / Create") { environment.openWorkspace(root: workspaceRoot) }
                        .disabled(client.state != .ready)
                    Button("Reconcile") { client.refreshWorkspace() }
                        .disabled(client.workspace == nil)
                    Button("Rebuild Index") { client.rebuildWorkspaceIndex() }
                        .disabled(client.workspace == nil)
                }
                if let workspace = client.workspace {
                    Text("\(workspace.root) · \(workspace.indexedFileCount) indexed files · event \(workspace.lastEventID.map(String.init) ?? "none")")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("STATUS.md").font(.caption.bold())
                            TextEditor(text: $workspaceStatus)
                                .font(.system(.caption, design: .monospaced))
                                .frame(minHeight: 86)
                                .overlay(RoundedRectangle(cornerRadius: 4).stroke(.quaternary))
                            HStack {
                                Button("Save Status") {
                                    client.writeWorkspaceFile(
                                        path: "STATUS.md",
                                        content: workspaceStatus
                                    )
                                }
                                Button("Load Context") { client.loadStatusContext() }
                                Button("Load Events") { client.loadWorkspaceEvents() }
                                if let context = client.workspaceContext {
                                    Text("\(context.files.count) context files")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                        }
                        VStack(alignment: .leading, spacing: 4) {
                            Text("References").font(.caption.bold())
                            HStack {
                                TextField("External directory", text: $referencePath)
                                Button("Add") {
                                    if client.addWorkspaceReference(path: referencePath) != nil {
                                        referencePath = ""
                                    }
                                }
                                .disabled(referencePath.isEmpty)
                            }
                            ForEach(workspace.references) { reference in
                                HStack {
                                    Text(reference.path)
                                        .font(.system(.caption, design: .monospaced))
                                        .lineLimit(1)
                                        .textSelection(.enabled)
                                    Spacer()
                                    Button("Remove", role: .destructive) {
                                        client.removeWorkspaceReference(id: reference.id)
                                    }
                                    .buttonStyle(.link)
                                }
                            }
                            if workspace.references.isEmpty {
                                Text("No external references")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            if !client.workspaceEvents.isEmpty {
                                Text("\(client.workspaceEvents.count) recent events loaded")
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                } else {
                    Text("Open a directory to create or resume its human-readable workspace.")
                        .foregroundStyle(.secondary)
                }
            }
        } label: {
            Text("Local-First Workspace")
        }
    }

    private var capturePanel: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(environment.captureShortcutStatus)
                        .font(.caption.monospaced())
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Open Quick Capture") { environment.showQuickCapture() }
                        .disabled(client.workspace == nil)
                    Button("Refresh") {
                        client.listCaptures()
                        client.loadCaptureMetrics()
                    }
                    .disabled(client.workspace == nil)
                }
                let metrics = client.captureMetrics
                Text(
                    "Today \(metrics.capturesSince) · completed \(metrics.completedCaptures) · abandoned \(metrics.abandonedCaptures) · corrections \(Double(metrics.correctionRateBasisPoints) / 100, specifier: "%.1f")% · median \(metrics.medianCompletionMilliseconds.map { "\($0) ms" } ?? "n/a")"
                )
                .font(.caption)
                .foregroundStyle(.secondary)

                ForEach(client.captures.prefix(5)) { capture in
                    HStack(alignment: .top) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(capture.title).lineLimit(1)
                            Text(
                                "\(capture.source.rawValue) · suggested \(capture.suggestedIntent.rawValue) · \(capture.routedPath ?? capture.status.rawValue)"
                            )
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Picker("Route", selection: Binding(
                            get: { capture.intent },
                            set: { client.correctCapture(id: capture.id, intent: $0) }
                        )) {
                            ForEach(CaptureIntent.allCases, id: \.self) { intent in
                                Text(intent.rawValue).tag(intent)
                            }
                        }
                        .labelsHidden()
                        .frame(width: 130)
                        .disabled(capture.status == .abandoned)
                    }
                }
                if client.captures.isEmpty {
                    Text("No captures yet")
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, minHeight: 50, alignment: .center)
                }
            }
        } label: {
            Text("Capture and Work Utility")
        }
    }

    private var currentStatePanel: some View {
        GroupBox {
            CurrentStateView(
                state: client.currentState,
                entryLimit: 8,
                reload: { client.loadCurrentState() }
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        } label: {
            Text("Work State Answers")
        }
    }

    private var avatarPanel: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Text(avatar.state.rawValue.capitalized)
                        .font(.headline)
                    Text(avatar.lastTransitionReason)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Text(avatar.isRuntimeEnabled ? "Runtime enabled" : "No renderer installed")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                HStack {
                    ForEach(AvatarState.allCases, id: \.self) { state in
                        Button(state.rawValue.capitalized) {
                            avatar.setManualState(state)
                        }
                        .buttonStyle(.link)
                    }
                    Button("Automatic") { avatar.setManualState(nil) }
                        .buttonStyle(.link)
                }
                if !avatar.errors.isEmpty {
                    ForEach(avatar.errors, id: \.self) { error in
                        Text(error)
                            .font(.caption.monospaced())
                            .foregroundStyle(.red)
                    }
                }
            }
        } label: {
            Text("Avatar Presence · Stage 07a")
        }
    }

    private var agentList: some View {
        GroupBox {
            List(client.agentRegistry.agents) { agent in
                HStack(alignment: .top, spacing: 12) {
                    Text(agent.status.rawValue)
                        .font(.caption.bold())
                        .foregroundStyle(statusColor(agent.status))
                        .frame(width: 105, alignment: .leading)
                    VStack(alignment: .leading, spacing: 3) {
                        HStack {
                            Text(agent.displayName).font(.headline)
                            Text(agent.adapter).font(.caption).foregroundStyle(.secondary)
                            if let version = agent.version {
                                Text(version).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        if let path = agent.executablePath {
                            Text(path).font(.system(.caption, design: .monospaced))
                                .textSelection(.enabled)
                        }
                        Text("Enforced mode: \(agent.enforcedSessionMode)")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                        if !agent.capabilities.isEmpty {
                            Text(agent.capabilities.joined(separator: " · "))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        if let diagnostic = agent.diagnostic {
                            Text(diagnostic).font(.caption).foregroundStyle(.orange)
                        }
                    }
                    Spacer()
                }
            }
            .frame(height: 190)
        } label: {
            HStack {
                Text("ACP Harnesses · generation \(client.agentRegistry.generation)")
                Spacer()
                if client.isRefreshingAgents {
                    ProgressView().controlSize(.small)
                }
                Button("Refresh") { client.refreshAgents() }
                    .buttonStyle(.link)
                    .disabled(client.state != .ready || client.isRefreshingAgents)
            }
        }
    }

    private func statusColor(_ status: AgentStatus) -> Color {
        switch status {
        case .ready: .green
        case .unavailable: .secondary
        case .unauthenticated: .orange
        case .incompatible, .failed: .red
        }
    }

    private var diagnosticList: some View {
        GroupBox("Diagnostics") {
            List(Array(client.diagnostics.enumerated()), id: \.offset) { _, message in
                Text(message).font(.system(.caption, design: .monospaced))
            }
        }
    }

    private var sessionPanel: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                HStack {
                    Picker("Agent", selection: $selectedAgentID) {
                        Text("Select a ready agent").tag("")
                        ForEach(client.agentRegistry.agents.filter { $0.status == .ready }) { agent in
                            Text(agent.displayName).tag(agent.id)
                        }
                    }
                    .frame(width: 260)
                    TextField("Working directory", text: $sessionCWD)
                    Button("Create Session") {
                        client.createSession(agentID: selectedAgentID, cwd: sessionCWD)
                    }
                    .disabled(selectedAgentID.isEmpty || client.isCreatingSession || client.activeRunID != nil)
                }
                HStack {
                    TextField("Prompt", text: $promptText)
                        .onSubmit { client.prompt(promptText) }
                    Button("Send") { client.prompt(promptText) }
                        .disabled(client.session == nil || client.activeRunID != nil)
                    Button("Cancel") { client.cancelPrompt() }
                        .disabled(client.activeRunID == nil)
                    Button("Clear") { client.clearSessionEvents() }.buttonStyle(.link)
                }
                if let session = client.session {
                    Text("Vela \(session.id) · ACP \(session.acpSessionID) · PID \(session.processID) · \(session.cwd)")
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                }
                List(client.sessionEvents) { event in
                    HStack(alignment: .top) {
                        Text("#\(event.sequence)").font(.caption.monospaced()).frame(width: 38, alignment: .trailing)
                        Text(eventDescription(event.payload))
                        Spacer()
                        Text(event.runID).font(.caption.monospaced()).foregroundStyle(.secondary)
                    }
                }
                .frame(height: 150)
            }
        } label: {
            Text("ACP Session Runtime")
        }
    }

    private var permissionPanel: some View {
        GroupBox {
            if client.pendingPermissions.isEmpty {
                Text("No pending permission requests")
                    .foregroundStyle(.secondary)
                    .frame(maxWidth: .infinity, minHeight: 90, alignment: .center)
            } else {
                List(client.pendingPermissions) { request in
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text(request.title).font(.headline)
                            Text(request.category.rawValue)
                                .font(.caption.monospaced())
                                .foregroundStyle(.orange)
                            Spacer()
                            Text(request.agentID).foregroundStyle(.secondary)
                        }
                        if let target = request.target {
                            Text(target)
                                .font(.system(.caption, design: .monospaced))
                                .textSelection(.enabled)
                        }
                        Text("\(request.sessionID) · \(request.runID) · \(request.id)")
                            .font(.caption.monospaced())
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                        HStack {
                            Button("Allow Once") {
                                client.resolvePermission(request, decision: .allowOnce)
                            }
                            .disabled(!request.canAllow)
                            Button("Allow for Session") {
                                client.resolvePermission(request, decision: .allowSession)
                            }
                            .disabled(!request.canAllow)
                            Button("Dismiss / Deny", role: .destructive) {
                                client.resolvePermission(request, decision: .deny)
                            }
                        }
                    }
                    .padding(.vertical, 4)
                }
                .frame(height: 150)
            }
        } label: {
            HStack {
                Text("Permission Broker")
                Spacer()
                Text("\(client.pendingPermissions.count) pending · \(client.permissionHistory.count) audited")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func eventDescription(_ payload: AgentEventPayload) -> String {
        switch payload {
        case let .textDelta(text): return text
        case let .planUpdated(entries): return "Plan: " + entries.map(\.content).joined(separator: " → ")
        case let .toolStarted(_, title): return "Tool started: \(title)"
        case let .toolFinished(id, status): return "Tool \(id): \(status)"
        case let .permissionRequested(request): return "Permission requested: \(request.title) [\(request.category.rawValue)]"
        case let .permissionResolved(record): return "Permission \(record.status.rawValue): \(record.request.title)"
        case let .usageUpdated(used, size): return "Usage: \(used) / \(size) tokens"
        case let .completed(reason): return "Completed: \(reason)"
        case .cancelled: return "Cancelled"
        case let .failed(code, message): return "Failed [\(code)]: \(message)"
        }
    }

    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "development"
    }
}
