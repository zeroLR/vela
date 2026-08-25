@preconcurrency import Network
import Combine
import Foundation

@MainActor
public final class IPCClient: ObservableObject {
    public enum State: String, Sendable {
        case disconnected
        case connecting
        case handshaking
        case ready
        case degraded
    }

    @Published public private(set) var state: State = .disconnected
    @Published public private(set) var transcript = StreamTranscript()
    @Published public private(set) var diagnostics: [String] = []
    @Published public private(set) var activeStreamRequestID: String?
    @Published public private(set) var agentRegistry = AgentRegistrySnapshot.empty
    @Published public private(set) var isRefreshingAgents = false
    @Published public private(set) var session: SessionDescriptor?
    @Published public private(set) var sessionEvents: [AgentEvent] = []
    @Published public private(set) var activeRunID: String?
    @Published public private(set) var isCreatingSession = false
    @Published public private(set) var pendingPermissions: [PermissionRequest] = []
    @Published public private(set) var permissionHistory: [PermissionAuditRecord] = []

    private var connection: NWConnection?
    private var receiveBuffer = Data()
    private var pendingMethods: [String: String] = [:]
    private var terminalRunIDs: Set<String> = []

    public init() {}

    public func connect(socketPath: String) {
        disconnect(reason: nil)
        state = .connecting
        appendDiagnostic("Connecting to \(socketPath)")

        let connection = NWConnection(to: .unix(path: socketPath), using: .tcp)
        self.connection = connection
        connection.stateUpdateHandler = { [weak self] networkState in
            Task { @MainActor [weak self] in
                self?.handle(networkState)
            }
        }
        connection.start(queue: DispatchQueue(label: "dev.vela.ipc"))
        receiveNext(on: connection)
    }

    public func disconnect(reason: String?) {
        connection?.stateUpdateHandler = nil
        connection?.cancel()
        connection = nil
        receiveBuffer.removeAll(keepingCapacity: true)
        pendingMethods.removeAll()
        activeStreamRequestID = nil
        isRefreshingAgents = false
        session = nil
        sessionEvents.removeAll()
        terminalRunIDs.removeAll()
        activeRunID = nil
        isCreatingSession = false
        pendingPermissions.removeAll()
        permissionHistory.removeAll()
        state = reason == nil ? .disconnected : .degraded
        if let reason {
            appendDiagnostic(reason)
        }
    }

    @discardableResult
    public func requestHealth() -> String? {
        send(method: "core.health")
    }

    @discardableResult
    public func listAgents() -> String? {
        send(method: "agents.list")
    }

    @discardableResult
    public func refreshAgents() -> String? {
        guard !isRefreshingAgents else { return nil }
        guard let id = send(method: "agents.refresh") else { return nil }
        isRefreshingAgents = true
        return id
    }

    @discardableResult
    public func createSession(agentID: String, cwd: String) -> String? {
        guard !isCreatingSession && activeRunID == nil else { return nil }
        guard let id = send(
            method: "session.create",
            params: ["agent_id": .string(agentID), "cwd": .string(cwd)]
        ) else { return nil }
        isCreatingSession = true
        return id
    }

    @discardableResult
    public func prompt(_ text: String) -> String? {
        guard let session, activeRunID == nil, !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return nil
        }
        return send(
            method: "session.prompt",
            params: ["session_id": .string(session.id), "text": .string(text)]
        )
    }

    @discardableResult
    public func cancelPrompt() -> String? {
        guard let session, let runID = activeRunID else { return nil }
        return send(
            method: "session.cancel",
            params: ["session_id": .string(session.id), "run_id": .string(runID)]
        )
    }

    @discardableResult
    public func resolvePermission(
        _ request: PermissionRequest,
        decision: PermissionDecision
    ) -> String? {
        guard pendingPermissions.contains(where: { $0.id == request.id }) else {
            appendDiagnostic("Permission request is no longer pending [\(request.id)]")
            return nil
        }
        return send(
            method: "permission.resolve",
            params: [
                "permission_id": .string(request.id),
                "session_id": .string(request.sessionID),
                "run_id": .string(request.runID),
                "decision": .string(decision.rawValue),
            ]
        )
    }

    @discardableResult
    public func listPendingPermissions() -> String? {
        guard let session else { return nil }
        return send(
            method: "permissions.pending",
            params: ["session_id": .string(session.id)]
        )
    }

    @discardableResult
    public func loadPermissionHistory() -> String? {
        guard let session else { return nil }
        return send(
            method: "permissions.history",
            params: ["session_id": .string(session.id)]
        )
    }

    public func clearSessionEvents() {
        sessionEvents.removeAll()
        terminalRunIDs.removeAll()
    }

    @discardableResult
    public func startStream(count: Int = 20, intervalMilliseconds: Int = 150) -> String? {
        guard activeStreamRequestID == nil else {
            appendDiagnostic("A stream is already active")
            return nil
        }
        guard let id = send(
            method: "stream.start",
            params: [
                "count": .number(Double(count)),
                "interval_ms": .number(Double(intervalMilliseconds)),
            ]
        ) else { return nil }
        activeStreamRequestID = id
        return id
    }

    @discardableResult
    public func cancelStream() -> String? {
        guard let targetID = activeStreamRequestID else { return nil }
        return send(
            method: "stream.cancel",
            params: ["target_request_id": .string(targetID)]
        )
    }

    public func clearEvents() {
        transcript.clear()
    }

    private func send(
        method: String,
        params: [String: JSONValue] = [:]
    ) -> String? {
        guard let connection, state == .ready || method == "core.hello" else {
            appendDiagnostic("Cannot send \(method): IPC is not ready")
            return nil
        }
        let id = UUID().uuidString.lowercased()
        let request = IPCRequest(id: id, method: method, params: params)
        do {
            let frame = try request.encodedFrame()
            pendingMethods[id] = method
            connection.send(content: frame, completion: .contentProcessed { [weak self] error in
                guard let error else { return }
                Task { @MainActor [weak self] in
                    self?.disconnect(reason: "Send failed for \(method): \(error)")
                }
            })
            return id
        } catch {
            appendDiagnostic("Could not encode \(method): \(error)")
            return nil
        }
    }

    private func handle(_ networkState: NWConnection.State) {
        switch networkState {
        case .ready:
            state = .handshaking
            appendDiagnostic("Socket connected; negotiating IPC v1")
            _ = send(method: "core.hello")
        case let .failed(error):
            disconnect(reason: "Connection failed: \(error)")
        case .cancelled:
            if state != .disconnected && state != .degraded {
                disconnect(reason: "Connection closed")
            }
        case let .waiting(error):
            appendDiagnostic("Connection waiting: \(error)")
        default:
            break
        }
    }

    private func receiveNext(on connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 64 * 1024) {
            [weak self, weak connection] data, _, isComplete, error in
            Task { @MainActor [weak self, weak connection] in
                guard let self, let connection, connection === self.connection else { return }
                if let data, !data.isEmpty {
                    self.consume(data)
                }
                if let error {
                    self.disconnect(reason: "Receive failed: \(error)")
                } else if isComplete {
                    self.disconnect(reason: "Core closed the IPC connection")
                } else {
                    self.receiveNext(on: connection)
                }
            }
        }
    }

    private func consume(_ data: Data) {
        receiveBuffer.append(data)
        while let newline = receiveBuffer.firstIndex(of: 0x0A) {
            let frame = receiveBuffer[..<newline]
            receiveBuffer.removeSubrange(...newline)
            guard !frame.isEmpty else { continue }
            do {
                let message = try JSONDecoder().decode(IPCMessage.self, from: frame)
                receive(message)
            } catch {
                appendDiagnostic("Malformed response from Core: \(error)")
            }
        }
    }

    private func receive(_ message: IPCMessage) {
        guard ProtocolVersion.current.isCompatible(with: message.version) else {
            disconnect(reason: "Core uses incompatible IPC major \(message.version.major)")
            return
        }

        if let eventName = message.event {
            if eventName == "agent.event" {
                receiveAgentEvent(message)
                return
            }
            let accepted = transcript.accept(message)
            if !accepted {
                appendDiagnostic("Ignored invalid or post-terminal event: \(eventName)")
            }
            if eventName == "stream.completed" || eventName == "stream.cancelled" {
                activeStreamRequestID = nil
            }
            return
        }

        guard let id = message.id else {
            appendDiagnostic("Received response without request ID")
            return
        }
        let method = pendingMethods.removeValue(forKey: id) ?? "unknown"
        if let error = message.error {
            appendDiagnostic("\(method) failed [\(error.code)]: \(error.message)")
            if method == "agents.refresh" {
                isRefreshingAgents = false
            }
            if method == "session.create" {
                isCreatingSession = false
            }
            if method == "core.hello" {
                state = .degraded
            }
        } else if method == "core.hello" {
            state = .ready
            appendDiagnostic("IPC v1 handshake complete")
            _ = refreshAgents()
        } else if method == "agents.list" || method == "agents.refresh" {
            if method == "agents.refresh" {
                isRefreshingAgents = false
            }
            guard let result = message.result, let snapshot = AgentRegistrySnapshot(result: result) else {
                appendDiagnostic("\(method) returned an invalid agent registry")
                return
            }
            if snapshot.generation >= agentRegistry.generation {
                agentRegistry = snapshot
            }
            appendDiagnostic("\(method) found \(snapshot.agents.count) agent definitions")
        } else if method == "session.create" {
            isCreatingSession = false
            guard let result = message.result, let descriptor = SessionDescriptor(result: result) else {
                appendDiagnostic("session.create returned an invalid session")
                return
            }
            session = descriptor
            sessionEvents.removeAll()
            terminalRunIDs.removeAll()
            pendingPermissions.removeAll()
            permissionHistory.removeAll()
            appendDiagnostic("session.create ready [\(descriptor.id), pid \(descriptor.processID)]")
            _ = listPendingPermissions()
            _ = loadPermissionHistory()
        } else if method == "session.prompt" {
            guard let runID = message.result?["run_id"]?.stringValue else {
                appendDiagnostic("session.prompt returned an invalid run")
                return
            }
            activeRunID = runID
            appendDiagnostic("session.prompt accepted [\(runID)]")
        } else if method == "permissions.pending" {
            guard let values = message.result?["permissions"]?.arrayValue else {
                appendDiagnostic("permissions.pending returned an invalid result")
                return
            }
            let permissions = values.compactMap(PermissionRequest.init(value:))
            guard permissions.count == values.count else {
                appendDiagnostic("permissions.pending returned malformed requests")
                return
            }
            pendingPermissions = permissions
        } else if method == "permissions.history" {
            guard let values = message.result?["records"]?.arrayValue else {
                appendDiagnostic("permissions.history returned an invalid result")
                return
            }
            let records = values.compactMap(PermissionAuditRecord.init(value:))
            guard records.count == values.count else {
                appendDiagnostic("permissions.history returned malformed records")
                return
            }
            permissionHistory = records
        } else if method == "permission.resolve" {
            guard let result = message.result,
                  let record = PermissionAuditRecord(value: .object(result)) else {
                appendDiagnostic("permission.resolve returned an invalid audit record")
                return
            }
            acceptPermissionRecord(record)
            appendDiagnostic("permission.resolve \(record.status.rawValue) [\(record.request.id)]")
        } else {
            appendDiagnostic("\(method) completed [\(id)]")
        }
    }

    private func receiveAgentEvent(_ message: IPCMessage) {
        guard let data = message.data, let event = AgentEvent(data: data) else {
            appendDiagnostic("Ignored malformed agent.event")
            return
        }
        guard !terminalRunIDs.contains(event.runID) else {
            appendDiagnostic("Ignored post-terminal agent event for \(event.runID)")
            return
        }
        sessionEvents.append(event)
        switch event.payload {
        case let .permissionRequested(request):
            if !pendingPermissions.contains(where: { $0.id == request.id }) {
                pendingPermissions.append(request)
                pendingPermissions.sort { $0.createdAtMilliseconds < $1.createdAtMilliseconds }
            }
        case let .permissionResolved(record):
            acceptPermissionRecord(record)
        default:
            break
        }
        if event.payload.isTerminal {
            terminalRunIDs.insert(event.runID)
            if activeRunID == event.runID {
                activeRunID = nil
            }
            pendingPermissions.removeAll { $0.runID == event.runID }
            appendDiagnostic("Agent run terminated [\(event.runID)]")
        }
    }

    private func acceptPermissionRecord(_ record: PermissionAuditRecord) {
        pendingPermissions.removeAll { $0.id == record.request.id }
        if !permissionHistory.contains(where: { $0.request.id == record.request.id }) {
            permissionHistory.append(record)
        }
    }

    private func appendDiagnostic(_ message: String) {
        diagnostics.append(message)
        if diagnostics.count > 100 {
            diagnostics.removeFirst(diagnostics.count - 100)
        }
    }
}
