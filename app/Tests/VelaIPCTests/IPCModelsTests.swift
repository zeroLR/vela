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
}
