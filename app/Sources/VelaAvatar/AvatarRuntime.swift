import Foundation

public enum AvatarState: String, CaseIterable, Equatable, Sendable {
    case idle
    case listening
    case thinking
    case speaking
    case success
    case error
}

public struct AvatarInputs: Equatable, Sendable {
    public var isListening: Bool
    public var hasError: Bool
    public var hasRecentTextDelta: Bool
    public var isThinking: Bool
    public var hasRecentSuccess: Bool
    public var lastSignalAt: Date?
    public var now: Date

    public init(
        isListening: Bool = false,
        hasError: Bool = false,
        hasRecentTextDelta: Bool = false,
        isThinking: Bool = false,
        hasRecentSuccess: Bool = false,
        lastSignalAt: Date? = nil,
        now: Date = .now
    ) {
        self.isListening = isListening
        self.hasError = hasError
        self.hasRecentTextDelta = hasRecentTextDelta
        self.isThinking = isThinking
        self.hasRecentSuccess = hasRecentSuccess
        self.lastSignalAt = lastSignalAt
        self.now = now
    }
}

public struct AvatarStateResolution: Equatable, Sendable {
    public let state: AvatarState
    public let reason: String

    public init(state: AvatarState, reason: String) {
        self.state = state
        self.reason = reason
    }
}

public enum AvatarStateReducer {
    public static let terminalDwell: TimeInterval = 4
    public static let stalledSignalTimeout: TimeInterval = 30

    public static func resolve(_ inputs: AvatarInputs) -> AvatarStateResolution {
        let age = inputs.lastSignalAt.map { inputs.now.timeIntervalSince($0) }
        guard let age, age <= stalledSignalTimeout else {
            return AvatarStateResolution(state: .idle, reason: "No recent runtime signal")
        }
        if inputs.isListening {
            return AvatarStateResolution(state: .listening, reason: "Push-to-talk is recording")
        }
        if inputs.hasError, age <= terminalDwell {
            return AvatarStateResolution(state: .error, reason: "Runtime reported an error")
        }
        if inputs.hasRecentTextDelta {
            return AvatarStateResolution(state: .speaking, reason: "Agent text is streaming")
        }
        if inputs.isThinking {
            return AvatarStateResolution(state: .thinking, reason: "Agent work is in progress")
        }
        if inputs.hasRecentSuccess, age <= terminalDwell {
            return AvatarStateResolution(state: .success, reason: "Agent run completed")
        }
        return AvatarStateResolution(state: .idle, reason: "No active avatar signal")
    }
}

@MainActor
public protocol AvatarRuntime: AnyObject {
    func load() throws
    func unload()
    func setState(_ state: AvatarState) throws
    func setExpression(_ expression: String?) throws
    func playMotion(_ motion: String) throws
    func setLipSync(_ value: Double) throws
    func lookAt(x: Double, y: Double) throws
}
