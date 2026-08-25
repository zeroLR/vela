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

    public var boolValue: Bool? {
        guard case let .bool(value) = self else { return nil }
        return value
    }

    public var objectValue: [String: JSONValue]? {
        guard case let .object(value) = self else { return nil }
        return value
    }

    public var arrayValue: [JSONValue]? {
        guard case let .array(value) = self else { return nil }
        return value
    }
}

public enum AgentStatus: String, Codable, Equatable, Sendable {
    case unavailable
    case ready
    case unauthenticated
    case incompatible
    case failed
}

public enum AgentSource: String, Codable, Equatable, Sendable {
    case builtIn = "built_in"
    case userDefined = "user_defined"
}

public struct AgentDescriptor: Identifiable, Codable, Equatable, Sendable {
    public let id: String
    public let displayName: String
    public let adapter: String
    public let source: AgentSource
    public let status: AgentStatus
    public let executablePath: String?
    public let version: String?
    public let protocolVersion: String?
    public let enforcedSessionMode: String
    public let capabilities: [String]
    public let authMethods: [String]
    public let diagnostic: String?

    enum CodingKeys: String, CodingKey {
        case id
        case displayName = "display_name"
        case adapter
        case source
        case status
        case executablePath = "executable_path"
        case version
        case protocolVersion = "protocol_version"
        case enforcedSessionMode = "enforced_session_mode"
        case capabilities
        case authMethods = "auth_methods"
        case diagnostic
    }

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let id = object["id"]?.stringValue,
            let displayName = object["display_name"]?.stringValue,
            let adapter = object["adapter"]?.stringValue,
            let sourceValue = object["source"]?.stringValue,
            let source = AgentSource(rawValue: sourceValue),
            let statusValue = object["status"]?.stringValue,
            let status = AgentStatus(rawValue: statusValue),
            let enforcedSessionMode = object["enforced_session_mode"]?.stringValue
        else { return nil }

        self.id = id
        self.displayName = displayName
        self.adapter = adapter
        self.source = source
        self.status = status
        executablePath = object["executable_path"]?.stringValue
        version = object["version"]?.stringValue
        protocolVersion = object["protocol_version"]?.stringValue
        self.enforcedSessionMode = enforcedSessionMode
        capabilities = object["capabilities"]?.arrayValue?.compactMap(\.stringValue) ?? []
        authMethods = object["auth_methods"]?.arrayValue?.compactMap(\.stringValue) ?? []
        diagnostic = object["diagnostic"]?.stringValue
    }
}

public struct AgentRegistrySnapshot: Equatable, Sendable {
    public static let empty = AgentRegistrySnapshot(generation: 0, refreshedAtMilliseconds: 0, agents: [])

    public let generation: UInt64
    public let refreshedAtMilliseconds: UInt64
    public let agents: [AgentDescriptor]

    public init(generation: UInt64, refreshedAtMilliseconds: UInt64, agents: [AgentDescriptor]) {
        self.generation = generation
        self.refreshedAtMilliseconds = refreshedAtMilliseconds
        self.agents = agents
    }

    init?(result: [String: JSONValue]) {
        guard
            let generationValue = result["generation"]?.numberValue,
            let generation = UInt64(exactly: generationValue),
            let refreshedAtValue = result["refreshed_at_ms"]?.numberValue,
            let refreshedAt = UInt64(exactly: refreshedAtValue),
            let agentValues = result["agents"]?.arrayValue
        else { return nil }
        let agents = agentValues.compactMap(AgentDescriptor.init(value:))
        guard agents.count == agentValues.count else { return nil }

        self.generation = generation
        refreshedAtMilliseconds = refreshedAt
        self.agents = agents
    }
}

public enum WorkspaceProvenance: String, Equatable, Sendable {
    case user, agent, tool, scheduler
    case externalFilesystem = "external_filesystem"
    case system
}

public struct WorkspaceReference: Identifiable, Equatable, Sendable {
    public let id: String
    public let path: String
    public let addedAtMilliseconds: UInt64

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let id = object["id"]?.stringValue,
            let path = object["path"]?.stringValue,
            let addedValue = object["added_at_ms"]?.numberValue,
            let addedAt = UInt64(exactly: addedValue)
        else { return nil }
        self.id = id
        self.path = path
        addedAtMilliseconds = addedAt
    }
}

public struct WorkspaceSnapshot: Equatable, Sendable {
    public let root: String
    public let createdAtMilliseconds: UInt64
    public let statusMarkdown: String
    public let inboxMarkdown: String
    public let references: [WorkspaceReference]
    public let indexedFileCount: UInt64
    public let lastEventID: UInt64?

    init?(result: [String: JSONValue]) {
        guard
            let root = result["root"]?.stringValue,
            let createdValue = result["created_at_ms"]?.numberValue,
            let createdAt = UInt64(exactly: createdValue),
            let statusMarkdown = result["status_markdown"]?.stringValue,
            let inboxMarkdown = result["inbox_markdown"]?.stringValue,
            let referenceValues = result["references"]?.arrayValue,
            let indexedValue = result["indexed_file_count"]?.numberValue,
            let indexedFileCount = UInt64(exactly: indexedValue)
        else { return nil }
        let references = referenceValues.compactMap(WorkspaceReference.init(value:))
        guard references.count == referenceValues.count else { return nil }
        self.root = root
        createdAtMilliseconds = createdAt
        self.statusMarkdown = statusMarkdown
        self.inboxMarkdown = inboxMarkdown
        self.references = references
        self.indexedFileCount = indexedFileCount
        lastEventID = result["last_event_id"]?.numberValue.flatMap(UInt64.init(exactly:))
    }
}

public struct WorkspaceEvent: Identifiable, Equatable, Sendable {
    public let id: UInt64
    public let timestampMilliseconds: UInt64
    public let kind: String
    public let path: String?
    public let provenance: WorkspaceProvenance
    public let correlationID: String?

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let idValue = object["id"]?.numberValue,
            let id = UInt64(exactly: idValue),
            let timestampValue = object["timestamp_ms"]?.numberValue,
            let timestamp = UInt64(exactly: timestampValue),
            let kind = object["kind"]?.stringValue,
            let provenanceValue = object["provenance"]?.stringValue,
            let provenance = WorkspaceProvenance(rawValue: provenanceValue)
        else { return nil }
        self.id = id
        timestampMilliseconds = timestamp
        self.kind = kind
        path = object["path"]?.stringValue
        self.provenance = provenance
        correlationID = object["correlation_id"]?.stringValue
    }
}

public struct WorkspaceContextFile: Equatable, Sendable {
    public let path: String
    public let content: String
    public let truncated: Bool

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let path = object["path"]?.stringValue,
            let content = object["content"]?.stringValue,
            let truncated = object["truncated"]?.boolValue
        else { return nil }
        self.path = path
        self.content = content
        self.truncated = truncated
    }
}

public struct WorkspaceContextSlice: Equatable, Sendable {
    public let scope: String
    public let files: [WorkspaceContextFile]

    init?(result: [String: JSONValue]) {
        guard
            let scope = result["scope"]?.stringValue,
            let fileValues = result["files"]?.arrayValue
        else { return nil }
        let files = fileValues.compactMap(WorkspaceContextFile.init(value:))
        guard files.count == fileValues.count else { return nil }
        self.scope = scope
        self.files = files
    }
}

public enum SessionState: String, Equatable, Sendable {
    case starting, ready, running, completed, cancelled, failed
}

public struct SessionDescriptor: Equatable, Sendable {
    public let id: String
    public let agentID: String
    public let acpSessionID: String
    public let processID: UInt32
    public let cwd: String
    public let state: SessionState

    init?(result: [String: JSONValue]) {
        guard
            let id = result["id"]?.stringValue,
            let agentID = result["agent_id"]?.stringValue,
            let acpSessionID = result["acp_session_id"]?.stringValue,
            let processValue = result["process_id"]?.numberValue,
            let processID = UInt32(exactly: processValue),
            let cwd = result["cwd"]?.stringValue,
            let stateValue = result["state"]?.stringValue,
            let state = SessionState(rawValue: stateValue)
        else { return nil }
        self.id = id
        self.agentID = agentID
        self.acpSessionID = acpSessionID
        self.processID = processID
        self.cwd = cwd
        self.state = state
    }
}

public struct SessionPlanEntry: Equatable, Sendable {
    public let content: String
    public let status: String
    public let priority: String
}

public enum PermissionCategory: String, Equatable, Sendable {
    case filesystemRead = "filesystem.read"
    case filesystemWrite = "filesystem.write"
    case shellExecute = "shell.execute"
    case networkOpenURL = "network.open_url"
    case mcpInvoke = "mcp.invoke"
    case other
}

public enum PermissionOptionKind: String, Equatable, Sendable {
    case allowOnce = "allow_once"
    case allowAlways = "allow_always"
    case rejectOnce = "reject_once"
    case rejectAlways = "reject_always"
}

public struct PermissionOption: Equatable, Sendable {
    public let id: String
    public let name: String
    public let kind: PermissionOptionKind

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let id = object["id"]?.stringValue,
            let name = object["name"]?.stringValue,
            let kindValue = object["kind"]?.stringValue,
            let kind = PermissionOptionKind(rawValue: kindValue)
        else { return nil }
        self.id = id
        self.name = name
        self.kind = kind
    }
}

public struct PermissionRequest: Identifiable, Equatable, Sendable {
    public let id: String
    public let agentID: String
    public let sessionID: String
    public let runID: String
    public let requestID: String
    public let toolCallID: String
    public let category: PermissionCategory
    public let title: String
    public let target: String?
    public let options: [PermissionOption]
    public let createdAtMilliseconds: UInt64
    public let expiresAtMilliseconds: UInt64

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let id = object["id"]?.stringValue,
            let agentID = object["agent_id"]?.stringValue,
            let sessionID = object["session_id"]?.stringValue,
            let runID = object["run_id"]?.stringValue,
            let requestID = object["request_id"]?.stringValue,
            let toolCallID = object["tool_call_id"]?.stringValue,
            let categoryValue = object["category"]?.stringValue,
            let category = PermissionCategory(rawValue: categoryValue),
            let title = object["title"]?.stringValue,
            let optionValues = object["options"]?.arrayValue,
            let createdValue = object["created_at_ms"]?.numberValue,
            let created = UInt64(exactly: createdValue),
            let expiresValue = object["expires_at_ms"]?.numberValue,
            let expires = UInt64(exactly: expiresValue)
        else { return nil }
        let options = optionValues.compactMap(PermissionOption.init(value:))
        guard options.count == optionValues.count else { return nil }
        self.id = id
        self.agentID = agentID
        self.sessionID = sessionID
        self.runID = runID
        self.requestID = requestID
        self.toolCallID = toolCallID
        self.category = category
        self.title = title
        target = object["target"]?.stringValue
        self.options = options
        createdAtMilliseconds = created
        expiresAtMilliseconds = expires
    }

    public var canAllow: Bool {
        options.contains { $0.kind == .allowOnce }
    }
}

public enum PermissionDecision: String, Equatable, Sendable {
    case allowOnce = "allow_once"
    case allowSession = "allow_session"
    case deny
}

public enum PermissionResolutionStatus: String, Equatable, Sendable {
    case allowed, denied
    case timedOut = "timed_out"
    case cancelled
}

public enum PermissionResolutionSource: String, Equatable, Sendable {
    case user
    case sessionGrant = "session_grant"
    case timeout, cancellation
}

public struct PermissionAuditRecord: Equatable, Sendable {
    public let request: PermissionRequest
    public let decision: PermissionDecision?
    public let status: PermissionResolutionStatus
    public let source: PermissionResolutionSource
    public let selectedOptionID: String?
    public let resolvedAtMilliseconds: UInt64

    init?(value: JSONValue) {
        guard
            let object = value.objectValue,
            let requestValue = object["request"],
            let request = PermissionRequest(value: requestValue),
            let statusValue = object["status"]?.stringValue,
            let status = PermissionResolutionStatus(rawValue: statusValue),
            let sourceValue = object["source"]?.stringValue,
            let source = PermissionResolutionSource(rawValue: sourceValue),
            let resolvedValue = object["resolved_at_ms"]?.numberValue,
            let resolved = UInt64(exactly: resolvedValue)
        else { return nil }
        self.request = request
        decision = object["decision"]?.stringValue.flatMap(PermissionDecision.init(rawValue:))
        self.status = status
        self.source = source
        selectedOptionID = object["selected_option_id"]?.stringValue
        resolvedAtMilliseconds = resolved
    }
}

public enum AgentEventPayload: Equatable, Sendable {
    case textDelta(String)
    case planUpdated([SessionPlanEntry])
    case toolStarted(id: String, title: String)
    case toolFinished(id: String, status: String)
    case permissionRequested(PermissionRequest)
    case permissionResolved(PermissionAuditRecord)
    case usageUpdated(used: UInt64, size: UInt64)
    case completed(stopReason: String)
    case cancelled
    case failed(code: String, message: String)

    public var isTerminal: Bool {
        switch self {
        case .completed, .cancelled, .failed: true
        default: false
        }
    }
}

public struct AgentEvent: Identifiable, Equatable, Sendable {
    public var id: String { "\(sessionID)-\(runID)-\(sequence)" }

    public let sessionID: String
    public let runID: String
    public let requestID: String
    public let sequence: UInt64
    public let timestampMilliseconds: UInt64
    public let payload: AgentEventPayload

    init?(data: [String: JSONValue]) {
        guard
            let sessionID = data["session_id"]?.stringValue,
            let runID = data["run_id"]?.stringValue,
            let requestID = data["request_id"]?.stringValue,
            let sequenceValue = data["sequence"]?.numberValue,
            let sequence = UInt64(exactly: sequenceValue),
            let timestampValue = data["timestamp_ms"]?.numberValue,
            let timestamp = UInt64(exactly: timestampValue),
            let kind = data["kind"]?.stringValue,
            let payload = Self.payload(kind: kind, data: data)
        else { return nil }
        self.sessionID = sessionID
        self.runID = runID
        self.requestID = requestID
        self.sequence = sequence
        timestampMilliseconds = timestamp
        self.payload = payload
    }

    private static func payload(kind: String, data: [String: JSONValue]) -> AgentEventPayload? {
        switch kind {
        case "text_delta":
            return data["text"]?.stringValue.map(AgentEventPayload.textDelta)
        case "plan_updated":
            let entries = data["entries"]?.arrayValue?.compactMap { value -> SessionPlanEntry? in
                guard
                    let entry = value.objectValue,
                    let content = entry["content"]?.stringValue,
                    let status = entry["status"]?.stringValue,
                    let priority = entry["priority"]?.stringValue
                else { return nil }
                return SessionPlanEntry(content: content, status: status, priority: priority)
            } ?? []
            return .planUpdated(entries)
        case "tool_started":
            guard let id = data["tool_call_id"]?.stringValue, let title = data["title"]?.stringValue else { return nil }
            return .toolStarted(id: id, title: title)
        case "tool_finished":
            guard let id = data["tool_call_id"]?.stringValue, let status = data["status"]?.stringValue else { return nil }
            return .toolFinished(id: id, status: status)
        case "permission_requested":
            guard let value = data["request"], let request = PermissionRequest(value: value) else { return nil }
            return .permissionRequested(request)
        case "permission_resolved":
            guard let value = data["record"], let record = PermissionAuditRecord(value: value) else { return nil }
            return .permissionResolved(record)
        case "usage_updated":
            guard
                let usedValue = data["used"]?.numberValue,
                let used = UInt64(exactly: usedValue),
                let sizeValue = data["size"]?.numberValue,
                let size = UInt64(exactly: sizeValue)
            else { return nil }
            return .usageUpdated(used: used, size: size)
        case "completed":
            return data["stop_reason"]?.stringValue.map { .completed(stopReason: $0) }
        case "cancelled": return .cancelled
        case "failed":
            guard let code = data["code"]?.stringValue, let message = data["message"]?.stringValue else { return nil }
            return .failed(code: code, message: message)
        default: return nil
        }
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
