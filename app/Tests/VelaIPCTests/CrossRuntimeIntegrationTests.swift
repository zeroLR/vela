import Darwin
import Foundation
import Testing
@testable import VelaIPC

@Suite("Swift to Rust vertical slice")
struct CrossRuntimeIntegrationTests {
    @Test("handshake, stream, Core exit, and explicit recovery")
    @MainActor
    func verticalSlice() async {
        guard ProcessInfo.processInfo.environment["VELA_CORE_PATH"] != nil else {
            return
        }

        let socketPath = "/tmp/vela-swift-test-\(getpid()).sock"
        let supervisor = CoreProcessSupervisor(socketPath: socketPath)
        let client = IPCClient()
        supervisor.onUnexpectedExit = { [weak client] description in
            client?.disconnect(reason: description)
        }
        defer {
            client.disconnect(reason: nil)
            supervisor.stop()
        }

        #expect(await supervisor.start())
        client.connect(socketPath: socketPath)
        #expect(await waitUntil { client.state == .ready })

        #expect(client.startStream(count: 3, intervalMilliseconds: 5) != nil)
        #expect(await waitUntil {
            client.activeStreamRequestID == nil && client.transcript.events.count == 4
        })
        #expect(client.transcript.events.map(\.name) == [
            "stream.chunk", "stream.chunk", "stream.chunk", "stream.completed",
        ])

        supervisor.killForDiagnostics()
        #expect(await waitUntil {
            supervisor.state == .degraded && client.state == .degraded
        })

        #expect(await supervisor.start())
        client.connect(socketPath: socketPath)
        #expect(await waitUntil { client.state == .ready })
        #expect(client.requestHealth() != nil)
        #expect(await waitUntil {
            client.diagnostics.contains { $0.hasPrefix("core.health completed") }
        })
    }

    @MainActor
    private func waitUntil(
        timeout: Duration = .seconds(3),
        condition: @escaping @MainActor () -> Bool
    ) async -> Bool {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while clock.now < deadline {
            if condition() { return true }
            try? await Task.sleep(for: .milliseconds(10))
        }
        return condition()
    }
}
