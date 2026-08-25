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
            from: Data(#"{"version":{"major":1,"minor":0},"id":"agents-1","result":{"generation":3,"refreshed_at_ms":42,"agents":[{"id":"codex","display_name":"Codex","adapter":"codex-acp","source":"built_in","status":"ready","executable_path":"/usr/local/bin/codex-acp","version":"0.8.1","protocol_version":"1","capabilities":["prompt.image","session.list"],"auth_methods":[],"diagnostic":null}]}}"#.utf8)
        )
        let result = try #require(message.result)
        let snapshot = try #require(AgentRegistrySnapshot(result: result))

        #expect(snapshot.generation == 3)
        #expect(snapshot.agents.count == 1)
        #expect(snapshot.agents[0].status == .ready)
        #expect(snapshot.agents[0].capabilities == ["prompt.image", "session.list"])
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
