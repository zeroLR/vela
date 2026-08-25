import Foundation

public struct ProtocolVersion: Codable, Equatable, Sendable {
    public static let current = ProtocolVersion(major: 1, minor: 0)

    public let major: UInt16
    public let minor: UInt16

    public init(major: UInt16, minor: UInt16) {
        self.major = major
        self.minor = minor
    }

    public func isCompatible(with other: ProtocolVersion) -> Bool {
        major == other.major
    }
}
public enum JSONValue: Codable, Equatable, Sendable {
    case string(String)
    case number(Double)
    case bool(Bool)
    case object([String: JSONValue])
    case array([JSONValue])
    case null

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() {
            self = .null
        } else if let value = try? container.decode(Bool.self) {
            self = .bool(value)
        } else if let value = try? container.decode(Double.self) {
            self = .number(value)
        } else if let value = try? container.decode(String.self) {
            self = .string(value)
        } else if let value = try? container.decode([String: JSONValue].self) {
            self = .object(value)
        } else {
            self = .array(try container.decode([JSONValue].self))
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case let .string(value): try container.encode(value)
        case let .number(value): try container.encode(value)
        case let .bool(value): try container.encode(value)
        case let .object(value): try container.encode(value)
        case let .array(value): try container.encode(value)
        case .null: try container.encodeNil()
        }
    }

    public var stringValue: String? {
        guard case let .string(value) = self else { return nil }
        return value
    }

    public var numberValue: Double? {
        guard case let .number(value) = self else { return nil }
        return value
    }
}

public struct IPCErrorPayload: Codable, Equatable, Sendable {
    public let code: String
    public let message: String
}

public struct IPCMessage: Codable, Equatable, Sendable {
    public let version: ProtocolVersion
    public let id: String?
    public let event: String?
    public let result: [String: JSONValue]?
    public let error: IPCErrorPayload?
    public let data: [String: JSONValue]?

    public init(
        version: ProtocolVersion,
        id: String? = nil,
        event: String? = nil,
        result: [String: JSONValue]? = nil,
        error: IPCErrorPayload? = nil,
        data: [String: JSONValue]? = nil
    ) {
        self.version = version
        self.id = id
        self.event = event
        self.result = result
        self.error = error
        self.data = data
    }
}

public struct IPCRequest: Encodable, Equatable, Sendable {
    public let version: ProtocolVersion
    public let id: String
    public let method: String
    public let params: [String: JSONValue]

    public init(
        id: String,
        method: String,
        params: [String: JSONValue] = [:],
        version: ProtocolVersion = .current
    ) {
        self.version = version
        self.id = id
        self.method = method
        self.params = params
    }

    public func encodedFrame() throws -> Data {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        var data = try encoder.encode(self)
        data.append(0x0A)
        return data
    }
}

public struct StreamEvent: Identifiable, Equatable, Sendable {
    public let id = UUID()
    public let name: String
    public let requestID: String
    public let sequence: Int?
    public let text: String?

    init?(message: IPCMessage) {
        guard
            let name = message.event,
            let requestID = message.data?["request_id"]?.stringValue
        else { return nil }
        self.name = name
        self.requestID = requestID
        self.sequence = message.data?["sequence"]?.numberValue.map(Int.init)
        self.text = message.data?["text"]?.stringValue
    }
}

public struct StreamTranscript: Sendable {
    public private(set) var events: [StreamEvent] = []
    private var terminalRequestIDs: Set<String> = []

    public init() {}

    @discardableResult
    public mutating func accept(_ message: IPCMessage) -> Bool {
        guard let event = StreamEvent(message: message) else { return false }
        guard !terminalRequestIDs.contains(event.requestID) else { return false }
        events.append(event)
        if event.name == "stream.completed" || event.name == "stream.cancelled" {
            terminalRequestIDs.insert(event.requestID)
        }
        return true
    }

    public mutating func clear() {
        events.removeAll()
        terminalRequestIDs.removeAll()
    }
}
