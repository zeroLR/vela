import SwiftUI
import VelaIPC

struct DiagnosticsView: View {
    let environment: AppEnvironment
    @ObservedObject private var client: IPCClient
    @ObservedObject private var supervisor: CoreProcessSupervisor

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

            HSplitView {
                eventList
                diagnosticList
            }
        }
        .padding(20)
        .frame(minWidth: 900, minHeight: 560)
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

    private var diagnosticList: some View {
        GroupBox("Diagnostics") {
            List(Array(client.diagnostics.enumerated()), id: \.offset) { _, message in
                Text(message).font(.system(.caption, design: .monospaced))
            }
        }
    }

    private var appVersion: String {
        Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "development"
    }
}
