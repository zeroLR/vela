import Darwin
import Foundation
import Testing
@testable import VelaIPC

@Suite("Swift to Rust vertical slice")
struct CrossRuntimeIntegrationTests {
    @Test("handshake, stream, Core exit, and explicit recovery")
    @MainActor
    func verticalSlice() async {
        guard let corePath = ProcessInfo.processInfo.environment["VELA_CORE_PATH"] else {
            return
        }

        let socketPath = "/tmp/vela-swift-test-\(getpid()).sock"
        let configPath = "/tmp/vela-swift-harnesses-\(getpid()).json"
        let fakeHarnessPath = URL(fileURLWithPath: corePath)
            .deletingLastPathComponent()
            .appendingPathComponent("fake-acp-harness").path
        let config: [String: Any] = ["harnesses": [[
            "id": "fake-swift",
            "display_name": "Fake Swift Agent",
            "command": fakeHarnessPath,
            "launch_arguments": ["--scenario", "ready"],
        ]]]
        guard
            FileManager.default.isExecutableFile(atPath: fakeHarnessPath),
            let configData = try? JSONSerialization.data(withJSONObject: config),
            (try? configData.write(to: URL(fileURLWithPath: configPath))) != nil
        else { return }
        let supervisor = CoreProcessSupervisor(
            socketPath: socketPath,
            environmentOverrides: ["VELA_HARNESS_CONFIG": configPath]
        )
        let client = IPCClient()
        supervisor.onUnexpectedExit = { [weak client] description in
            client?.disconnect(reason: description)
        }
        defer {
            client.disconnect(reason: nil)
            supervisor.stop()
            try? FileManager.default.removeItem(atPath: configPath)
        }

        #expect(await supervisor.start())
        client.connect(socketPath: socketPath)
        #expect(await waitUntil { client.state == .ready })
        #expect(await waitUntil {
            client.agentRegistry.agents.contains { $0.id == "fake-swift" && $0.status == .ready }
        })

        #expect(client.createSession(agentID: "fake-swift", cwd: FileManager.default.currentDirectoryPath) != nil)
        #expect(await waitUntil { client.session?.agentID == "fake-swift" })
        #expect(client.prompt("hello") != nil)
        #expect(await waitUntil {
            client.sessionEvents.last?.payload.isTerminal == true
        })
        #expect(client.sessionEvents.contains {
            if case .textDelta = $0.payload { true } else { false }
        })

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
