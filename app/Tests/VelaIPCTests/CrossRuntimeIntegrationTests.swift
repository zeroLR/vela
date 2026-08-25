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
        let workspacePath = "/tmp/vela-swift-workspace-\(getpid())"
        let referencePath = "/tmp/vela-swift-reference-\(getpid())"
        let fakeHarnessPath = URL(fileURLWithPath: corePath)
            .deletingLastPathComponent()
            .appendingPathComponent("fake-acp-harness").path
        let config: [String: Any] = ["harnesses": [
            [
                "id": "fake-swift",
                "display_name": "Fake Swift Agent",
                "command": fakeHarnessPath,
                "enforced_session_mode": "safe",
                "launch_arguments": ["--scenario", "ready"],
            ],
            [
                "id": "fake-permission",
                "display_name": "Fake Permission Agent",
                "command": fakeHarnessPath,
                "enforced_session_mode": "safe",
                "launch_arguments": ["--scenario", "permission", "--permission-kind", "edit"],
            ],
        ]]
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
            try? FileManager.default.removeItem(atPath: workspacePath)
            try? FileManager.default.removeItem(atPath: referencePath)
        }

        #expect(await supervisor.start())
        client.connect(socketPath: socketPath)
        #expect(await waitUntil { client.state == .ready })
        #expect(await waitUntil {
            client.agentRegistry.agents.contains { $0.id == "fake-swift" && $0.status == .ready }
        })
        #expect(client.agentRegistry.agents.contains {
            $0.id == "fake-permission" && $0.status == .ready
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

        #expect(client.createSession(agentID: "fake-permission", cwd: FileManager.default.currentDirectoryPath) != nil)
        #expect(await waitUntil { client.session?.agentID == "fake-permission" })
        #expect(client.prompt("request permission") != nil)
        #expect(await waitUntil { client.pendingPermissions.count == 1 })
        guard let permission = client.pendingPermissions.first else {
            Issue.record("expected a pending permission request")
            return
        }
        #expect(client.resolvePermission(permission, decision: .allowOnce) != nil)
        #expect(await waitUntil {
            client.pendingPermissions.isEmpty && client.sessionEvents.last?.payload.isTerminal == true
        })
        #expect(client.permissionHistory.contains {
            $0.request.id == permission.id && $0.status == .allowed
        })

        #expect(client.startStream(count: 3, intervalMilliseconds: 5) != nil)
        #expect(await waitUntil {
            client.activeStreamRequestID == nil && client.transcript.events.count == 4
        })
        #expect(client.transcript.events.map(\.name) == [
            "stream.chunk", "stream.chunk", "stream.chunk", "stream.completed",
        ])

        try? FileManager.default.createDirectory(
            atPath: referencePath,
            withIntermediateDirectories: true
        )
        let externalFile = URL(fileURLWithPath: referencePath).appendingPathComponent("README.md")
        try? Data("external truth\n".utf8).write(to: externalFile)
        #expect(client.openWorkspace(root: workspacePath) != nil)
        #expect(await waitUntil { client.workspace != nil })
        #expect(client.workspace?.root.hasSuffix("/vela-swift-workspace-\(getpid())") == true)
        #expect(FileManager.default.fileExists(atPath: workspacePath + "/STATUS.md"))

        #expect(client.writeWorkspaceFile(
            path: "STATUS.md",
            content: "# Status\n\nSwift vertical slice\n",
            provenance: .user
        ) != nil)
        #expect(await waitUntil {
            client.workspace?.statusMarkdown.contains("Swift vertical slice") == true
        })
        #expect(client.addWorkspaceReference(path: referencePath) != nil)
        #expect(await waitUntil { client.workspace?.references.count == 1 })
        #expect(client.loadStatusContext() != nil)
        #expect(await waitUntil { client.workspaceContext?.files.count == 2 })
        #expect(client.loadWorkspaceEvents() != nil)
        #expect(await waitUntil {
            client.workspaceEvents.contains {
                $0.kind == "workspace.file_changed" && $0.provenance == .user
            }
        })

        let captureStartedAt = UInt64(Date().timeIntervalSince1970 * 1_000) - 25
        #expect(client.submitCapture(
            rawText: "  idea: simplify the quick panel  ",
            source: .text,
            startedAtMilliseconds: captureStartedAt
        ) != nil)
        #expect(await waitUntil { client.captures.first?.suggestedIntent == .idea })
        guard let capture = client.captures.first else {
            Issue.record("expected a completed capture")
            return
        }
        #expect(capture.rawText == "  idea: simplify the quick panel  ")
        #expect(client.correctCapture(id: capture.id, intent: .todo) != nil)
        #expect(await waitUntil {
            client.captures.first(where: { $0.id == capture.id })?.intent == .todo
        })
        #expect(client.abandonCapture(
            rawText: "partial speech transcript",
            source: .speech,
            startedAtMilliseconds: captureStartedAt
        ) != nil)
        #expect(await waitUntil {
            client.captureMetrics.totalCaptures == 2
                && client.captureMetrics.abandonedCaptures == 1
                && client.captureMetrics.correctedCaptures == 1
        })
        let correctedCapture = client.captures.first { $0.id == capture.id }
        #expect(correctedCapture?.routedPath?.hasPrefix("tasks/") == true)
        if let routedPath = correctedCapture?.routedPath {
            #expect(FileManager.default.fileExists(
                atPath: workspacePath + "/" + routedPath
            ))
        }

        guard let referenceID = client.workspace?.references.first?.id else {
            Issue.record("expected a workspace reference")
            return
        }
        #expect(client.removeWorkspaceReference(id: referenceID) != nil)
        #expect(await waitUntil { client.workspace?.references.isEmpty == true })
        #expect(FileManager.default.fileExists(atPath: externalFile.path))

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
