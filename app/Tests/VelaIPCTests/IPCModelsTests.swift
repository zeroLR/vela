import Foundation
import Testing
@testable import VelaIPC

@Suite("IPC v1 models")
struct IPCModelsTests {
    @Test("request frames are newline-delimited and versioned")
    func requestEncoding() throws {
        let request = IPCRequest(
            id: "health-1",
            method: "core.health",
            params: [:]
        )
        let frame = try request.encodedFrame()
        #expect(frame.last == 0x0A)

        let object = try #require(
            JSONSerialization.jsonObject(with: frame.dropLast()) as? [String: Any]
        )
        let version = try #require(object["version"] as? [String: Any])
        #expect(version["major"] as? Int == 1)
        #expect(object["id"] as? String == "health-1")
        #expect(object["method"] as? String == "core.health")
    }

    @Test("post-terminal stream events are rejected")
    func terminalInvariant() throws {
        let decoder = JSONDecoder()
        let chunk = try decoder.decode(IPCMessage.self, from: Data(#"{"version":{"major":1,"minor":0},"event":"stream.chunk","data":{"request_id":"stream-1","sequence":1,"text":"one"}}"#.utf8))
        let terminal = try decoder.decode(IPCMessage.self, from: Data(#"{"version":{"major":1,"minor":0},"event":"stream.completed","data":{"request_id":"stream-1","emitted":1}}"#.utf8))
        let lateChunk = try decoder.decode(IPCMessage.self, from: Data(#"{"version":{"major":1,"minor":0},"event":"stream.chunk","data":{"request_id":"stream-1","sequence":2,"text":"late"}}"#.utf8))
        var transcript = StreamTranscript()

        let acceptedChunk = transcript.accept(chunk)
        let acceptedTerminal = transcript.accept(terminal)
        let acceptedLateChunk = transcript.accept(lateChunk)
        #expect(acceptedChunk)
        #expect(acceptedTerminal)
        #expect(!acceptedLateChunk)
        #expect(transcript.events.count == 2)
    }

    @Test("major versions determine compatibility")
    func compatibility() {
        #expect(ProtocolVersion.current.isCompatible(with: ProtocolVersion(major: 1, minor: 9)))
        #expect(!ProtocolVersion.current.isCompatible(with: ProtocolVersion(major: 2, minor: 0)))
    }

    @Test("agent registry responses preserve normalized discovery fields")
    func agentRegistry() throws {
        let message = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data(#"{"version":{"major":1,"minor":0},"id":"agents-1","result":{"generation":3,"refreshed_at_ms":42,"agents":[{"id":"codex","display_name":"Codex","adapter":"codex-acp","source":"built_in","status":"ready","executable_path":"/usr/local/bin/codex-acp","version":"0.8.1","protocol_version":"1","enforced_session_mode":"read-only","capabilities":["prompt.image","session.list"],"auth_methods":[],"diagnostic":null}]}}"#.utf8)
        )
        let result = try #require(message.result)
        let snapshot = try #require(AgentRegistrySnapshot(result: result))

        #expect(snapshot.generation == 3)
        #expect(snapshot.agents.count == 1)
        #expect(snapshot.agents[0].status == .ready)
        #expect(snapshot.agents[0].enforcedSessionMode == "read-only")
        #expect(snapshot.agents[0].capabilities == ["prompt.image", "session.list"])
    }

    @Test("workspace snapshots and bounded context decode without SQLite details")
    func workspaceModels() throws {
        let snapshotMessage = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data(##"{"version":{"major":1,"minor":0},"id":"workspace-1","result":{"root":"/tmp/vela","created_at_ms":42,"status_markdown":"# Status\n","inbox_markdown":"# Inbox\n","references":[{"id":"reference-1","path":"/tmp/source","added_at_ms":43}],"indexed_file_count":7,"last_event_id":9}}"##.utf8)
        )
        let snapshot = try #require(snapshotMessage.result.flatMap(WorkspaceSnapshot.init(result:)))
        #expect(snapshot.root == "/tmp/vela")
        #expect(snapshot.references.first?.id == "reference-1")
        #expect(snapshot.indexedFileCount == 7)

        let contextMessage = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data(##"{"version":{"major":1,"minor":0},"id":"context-1","result":{"scope":"status","files":[{"path":"STATUS.md","content":"# Status\n","truncated":false}]}}"##.utf8)
        )
        let context = try #require(contextMessage.result.flatMap(WorkspaceContextSlice.init(result:)))
        #expect(context.scope == "status")
        #expect(context.files.count == 1)
        #expect(context.files.first?.path == "STATUS.md")
        #expect(context.files.first?.content == "# Status\n")
        #expect(context.files.first?.truncated == false)
    }

    @Test("capture records retain raw routing and correction fields")
    func captureModels() throws {
        let message = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data(#"{"version":{"major":1,"minor":0},"id":"capture-1","result":{"id":"capture-42","source":"speech","status":"completed","raw_text":"  todo: ship  ","normalized_text":"todo: ship","suggested_intent":"todo","intent":"note","title":"todo: ship","routed_path":"notes/capture-42.md","started_at_ms":40,"completed_at_ms":50,"correction_count":1}}"#.utf8)
        )
        let capture = try #require(message.result.flatMap { CaptureRecord(value: .object($0)) })
        #expect(capture.source == .speech)
        #expect(capture.rawText == "  todo: ship  ")
        #expect(capture.suggestedIntent == .todo)
        #expect(capture.intent == .note)
        #expect(capture.correctionCount == 1)

        let metrics = try #require(CaptureMetrics(result: [
            "total_captures": .number(3),
            "completed_captures": .number(2),
            "abandoned_captures": .number(1),
            "captures_since": .number(3),
            "corrected_captures": .number(1),
            "correction_rate_basis_points": .number(5_000),
            "median_completion_ms": .number(20),
        ]))
        #expect(metrics.correctionRateBasisPoints == 5_000)
        #expect(metrics.medianCompletionMilliseconds == 20)
    }

    @Test("normalized agent events decode without ACP wire types")
    func agentEvent() throws {
        let message = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data(#"{"version":{"major":1,"minor":0},"event":"agent.event","data":{"session_id":"session-1","run_id":"run-1","request_id":"request-1","sequence":2,"timestamp_ms":42,"kind":"text_delta","text":"hello"}}"#.utf8)
        )
        let data = try #require(message.data)
        let event = try #require(AgentEvent(data: data))
        #expect(event.sequence == 2)
        #expect(event.payload == .textDelta("hello"))
        #expect(!event.payload.isTerminal)
    }

    @Test("permission requests and audit records decode as Vela models")
    func permissionEvents() throws {
        let request = #"{"id":"permission-1","agent_id":"fake","session_id":"session-1","run_id":"run-1","request_id":"prompt-1","tool_call_id":"tool-1","category":"filesystem.write","title":"Write file","target":"/tmp/a.txt","options":[{"id":"allow","name":"Allow once","kind":"allow_once"}],"created_at_ms":40,"expires_at_ms":80}"#
        let requested = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data((#"{"version":{"major":1,"minor":0},"event":"agent.event","data":{"session_id":"session-1","run_id":"run-1","request_id":"prompt-1","sequence":1,"timestamp_ms":42,"kind":"permission_requested","request":"# + request + "}}").utf8)
        )
        let requestedData = try #require(requested.data)
        let requestedEvent = try #require(AgentEvent(data: requestedData))
        guard case let .permissionRequested(permission) = requestedEvent.payload else {
            Issue.record("expected permission request")
            return
        }
        #expect(permission.category == .filesystemWrite)
        #expect(permission.canAllow)

        let resolved = try JSONDecoder().decode(
            IPCMessage.self,
            from: Data((#"{"version":{"major":1,"minor":0},"event":"agent.event","data":{"session_id":"session-1","run_id":"run-1","request_id":"prompt-1","sequence":2,"timestamp_ms":43,"kind":"permission_resolved","record":{"request":"# + request + #", "decision":"allow_once","status":"allowed","source":"user","selected_option_id":"allow","resolved_at_ms":43}}}"#).utf8)
        )
        let resolvedData = try #require(resolved.data)
        let resolvedEvent = try #require(AgentEvent(data: resolvedData))
        guard case let .permissionResolved(record) = resolvedEvent.payload else {
            Issue.record("expected permission audit record")
            return
        }
        #expect(record.status == .allowed)
        #expect(record.decision == .allowOnce)
    }
}
