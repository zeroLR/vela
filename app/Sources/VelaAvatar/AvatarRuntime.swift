import Foundation
import VelaIPC

public enum AvatarState: String, CaseIterable, Codable, Equatable, Sendable {
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
    var isPresentationVisible: Bool { get }
    var lastRenderedAt: Date? { get }
    func load() throws
    func unload()
    func setState(_ state: AvatarState) throws
    func setExpression(_ expression: String?) throws
    func playMotion(_ motion: String) throws
    func setLipSync(_ value: Double) throws
    func lookAt(x: Double, y: Double) throws
}

public extension AvatarRuntime {
    var isPresentationVisible: Bool { false }
    var lastRenderedAt: Date? { nil }
}

@MainActor
public final class AvatarController: ObservableObject {
    public static let renderHeartbeatTimeout: TimeInterval = 2

    @Published public private(set) var state: AvatarState = .idle
    @Published public private(set) var lastTransitionReason = "No runtime signal"
    @Published public private(set) var errors: [String] = []
    @Published public private(set) var isRuntimeEnabled = false
    @Published public private(set) var isLipSyncEnabled = true
    @Published public private(set) var isMicrophoneLipSyncEnabled = true
    @Published public private(set) var isTextLipSyncEnabled = true

    private static let textDeltaWindow: TimeInterval = 1.5

    private let client: IPCClient?
    private let now: () -> Date
    private let mapping: AvatarMappingConfiguration
    private let microphoneLipSync = MicrophoneRMSSource()
    private let textLipSync = TextCadenceLipSyncSource()
    private var runtime: (any AvatarRuntime)?
    private var manualState: AvatarState?
    private var manualStateAt: Date?
    private var isListening = false
    private var listeningSignalAt: Date?
    private var lastClientSignature = ""
    private var lastClientSignalAt: Date?
    private var lastTextEventID: String?
    private var renderFailureAt: Date?
    private var watchdogTask: Task<Void, Never>?

    public init(
        client: IPCClient? = nil,
        runtime: (any AvatarRuntime)? = nil,
        mapping: AvatarMappingConfiguration = .builtIn,
        configurationDiagnostic: String? = nil,
        now: @escaping () -> Date = { .now }
    ) {
        self.client = client
        self.mapping = mapping
        self.now = now
        if let configurationDiagnostic {
            errors.append(configurationDiagnostic)
        }
        install(runtime)
        watchdogTask = Task { @MainActor [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(1))
                guard !Task.isCancelled else { return }
                self?.refresh()
            }
        }
        refresh()
    }

    deinit {
        watchdogTask?.cancel()
    }

    public func setListening(_ isListening: Bool) {
        self.isListening = isListening
        listeningSignalAt = isListening ? now() : nil
        manualState = nil
        manualStateAt = nil
        refresh()
    }

    public func setManualState(_ state: AvatarState?) {
        manualState = state
        manualStateAt = state == nil ? nil : now()
        refresh()
    }

    public func setLipSyncEnabled(_ isEnabled: Bool) {
        isLipSyncEnabled = isEnabled
        driveLipSync(at: now())
    }

    public func setMicrophoneLipSyncEnabled(_ isEnabled: Bool) {
        isMicrophoneLipSyncEnabled = isEnabled
        microphoneLipSync.isEnabled = isEnabled
        driveLipSync(at: now())
    }

    public func setTextLipSyncEnabled(_ isEnabled: Bool) {
        isTextLipSyncEnabled = isEnabled
        textLipSync.isEnabled = isEnabled
        driveLipSync(at: now())
    }

    public func setMicrophoneRMS(_ value: Double) {
        let currentTime = now()
        microphoneLipSync.accept(value, at: currentTime)
        driveLipSync(at: currentTime)
    }

    public func install(_ runtime: (any AvatarRuntime)?) {
        self.runtime?.unload()
        self.runtime = runtime
        guard let runtime else {
            isRuntimeEnabled = false
            return
        }
        do {
            try runtime.load()
            isRuntimeEnabled = true
            try runtime.setState(state)
            try applyMapping(for: state, to: runtime)
        } catch {
            disableRuntime(error)
        }
    }

    public func refresh() {
        let currentTime = now()
        if let renderHealthResolution = renderHealthResolution(at: currentTime) {
            apply(renderHealthResolution)
            return
        }
        if let manualState, let manualStateAt {
            apply(AvatarStateReducer.resolve(manualInputs(manualState, at: manualStateAt, now: currentTime)))
        } else {
            apply(AvatarStateReducer.resolve(clientInputs(at: currentTime)))
        }
    }

    private func manualInputs(_ state: AvatarState, at signalAt: Date, now: Date) -> AvatarInputs {
        AvatarInputs(
            isListening: state == .listening,
            hasError: state == .error,
            hasRecentTextDelta: state == .speaking,
            isThinking: state == .thinking,
            hasRecentSuccess: state == .success,
            lastSignalAt: signalAt,
            now: now
        )
    }

    private func renderHealthResolution(at now: Date) -> AvatarStateResolution? {
        guard let runtime, isRuntimeEnabled, runtime.isPresentationVisible else {
            renderFailureAt = nil
            return nil
        }
        if let lastRenderedAt = runtime.lastRenderedAt,
           now.timeIntervalSince(lastRenderedAt) <= Self.renderHeartbeatTimeout {
            renderFailureAt = nil
            return nil
        }
        if renderFailureAt == nil {
            renderFailureAt = now
        }
        guard let renderFailureAt else { return nil }
        if now.timeIntervalSince(renderFailureAt) <= AvatarStateReducer.terminalDwell {
            return AvatarStateResolution(state: .error, reason: "Avatar renderer stopped producing frames")
        }
        return AvatarStateResolution(state: .idle, reason: "Avatar renderer watchdog timed out")
    }

    private func clientInputs(at now: Date) -> AvatarInputs {
        guard let client else {
            return AvatarInputs(
                isListening: isListening,
                lastSignalAt: listeningSignalAt,
                now: now
            )
        }
        let signature = [
            client.state.rawValue,
            client.activeRunID ?? "",
            String(client.pendingPermissions.count),
            client.sessionEvents.last?.id ?? "",
        ].joined(separator: "|")
        if signature != lastClientSignature {
            lastClientSignature = signature
            lastClientSignalAt = now
        }

        let events = client.sessionEvents
        let lastText = events.last { event in
            if case .textDelta = event.payload { return true }
            return false
        }
        if let lastText, lastText.id != lastTextEventID {
            lastTextEventID = lastText.id
            textLipSync.recordTextDelta(at: eventDate(lastText))
        }
        let lastTextAt = lastText.map(eventDate)
        let lastSuccessAt = events.last { event in
            if case .completed = event.payload { return true }
            return false
        }.map(eventDate)
        let lastErrorAt = events.last { event in
            if case .failed = event.payload { return true }
            return false
        }.map(eventDate)
        let hasActiveTool = activeToolExists(in: events)
        let recentText = lastTextAt.map { now.timeIntervalSince($0) <= Self.textDeltaWindow } ?? false
        let recentSuccess = lastSuccessAt.map { now.timeIntervalSince($0) <= AvatarStateReducer.terminalDwell } ?? false
        let recentError = lastErrorAt.map { now.timeIntervalSince($0) <= AvatarStateReducer.terminalDwell } ?? false
        let lastSignalAt = [listeningSignalAt, lastClientSignalAt, lastTextAt, lastSuccessAt, lastErrorAt]
            .compactMap { $0 }
            .max()

        return AvatarInputs(
            isListening: isListening,
            hasError: client.state == .degraded || recentError,
            hasRecentTextDelta: recentText,
            isThinking: client.activeRunID != nil || !client.pendingPermissions.isEmpty || hasActiveTool,
            hasRecentSuccess: recentSuccess,
            lastSignalAt: lastSignalAt,
            now: now
        )
    }

    private func activeToolExists(in events: [AgentEvent]) -> Bool {
        var activeIDs: Set<String> = []
        for event in events {
            switch event.payload {
            case let .toolStarted(id, _): activeIDs.insert(id)
            case let .toolFinished(id, _): activeIDs.remove(id)
            default: break
            }
        }
        return !activeIDs.isEmpty
    }

    private func eventDate(_ event: AgentEvent) -> Date {
        Date(timeIntervalSince1970: TimeInterval(event.timestampMilliseconds) / 1_000)
    }

    private func apply(_ resolution: AvatarStateResolution) {
        if state != resolution.state || lastTransitionReason != resolution.reason {
            state = resolution.state
            lastTransitionReason = resolution.reason
            if let runtime, isRuntimeEnabled {
                do {
                    try runtime.setState(resolution.state)
                    try applyMapping(for: resolution.state, to: runtime)
                } catch {
                    disableRuntime(error)
                    return
                }
            }
        }
        driveLipSync(at: now())
    }

    private func applyMapping(for state: AvatarState, to runtime: any AvatarRuntime) throws {
        try runtime.setExpression(mapping.stateExpressions[state])
        if let motion = mapping.stateMotions[state] {
            try runtime.playMotion(motion)
        }
    }

    private func driveLipSync(at now: Date) {
        guard let runtime, isRuntimeEnabled else { return }
        let value: Double
        if !isLipSyncEnabled {
            value = 0
        } else {
            switch state {
            case .listening: value = microphoneLipSync.value(at: now)
            case .speaking: value = textLipSync.value(at: now)
            default: value = 0
            }
        }
        do {
            try runtime.setLipSync(value)
        } catch {
            disableRuntime(error)
        }
    }

    private func disableRuntime(_ error: Error) {
        runtime?.unload()
        runtime = nil
        isRuntimeEnabled = false
        let message = "Avatar runtime disabled: \(error.localizedDescription)"
        errors.append(message)
        if errors.count > 20 {
            errors.removeFirst(errors.count - 20)
        }
        state = .idle
        lastTransitionReason = message
    }
}
