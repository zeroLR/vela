import SwiftUI
import VelaIPC

struct DiagnosticsView: View {
    let environment: AppEnvironment
    @ObservedObject private var client: IPCClient
    @ObservedObject private var supervisor: CoreProcessSupervisor
    @State private var selectedAgentID = ""
    @State private var sessionCWD = FileManager.default.currentDirectoryPath
    @State private var promptText = "Summarize this workspace briefly."

    init(environment: AppEnvironment) {
        self.environment = environment
        client = environment.client
        supervisor = environment.supervisor
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
                Button("Start 20-event Stream") { client.startStream() }
                    .disabled(client.state != .ready || client.activeStreamRequestID != nil)
                Button("Cancel Stream") { client.cancelStream() }
                    .disabled(client.activeStreamRequestID == nil)
                Button("Kill Core") { supervisor.killForDiagnostics() }
                    .disabled(supervisor.state != .running)
            }

            agentList
            sessionPanel

            HSplitView {
                eventList
                diagnosticList
            }
        }
        .padding(20)
        .frame(minWidth: 900, minHeight: 900)
        .onChange(of: client.agentRegistry.agents) { _, agents in
            if !agents.contains(where: { $0.id == selectedAgentID && $0.status == .ready }) {
                selectedAgentID = agents.first(where: { $0.status == .ready })?.id ?? ""
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

    private func eventDescription(_ payload: AgentEventPayload) -> String {
        switch payload {
        case let .textDelta(text): return text
        case let .planUpdated(entries): return "Plan: " + entries.map(\.content).joined(separator: " → ")
        case let .toolStarted(_, title): return "Tool started: \(title)"
        case let .toolFinished(id, status): return "Tool \(id): \(status)"
        case let .permissionRequested(_, title, _): return "Permission requested: \(title ?? "unknown tool") (safely cancelled)"
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
