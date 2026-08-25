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

    private var connection: NWConnection?
    private var receiveBuffer = Data()
    private var pendingMethods: [String: String] = [:]

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
            if method == "core.hello" {
                state = .degraded
            }
        } else if method == "core.hello" {
            state = .ready
            appendDiagnostic("IPC v1 handshake complete")
        } else {
            appendDiagnostic("\(method) completed [\(id)]")
        }
    }

    private func appendDiagnostic(_ message: String) {
        diagnostics.append(message)
        if diagnostics.count > 100 {
            diagnostics.removeFirst(diagnostics.count - 100)
        }
    }
}
