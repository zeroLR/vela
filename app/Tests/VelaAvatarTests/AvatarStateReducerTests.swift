import Foundation
import Testing
@testable import VelaAvatar

struct AvatarStateReducerTests {
    private let now = Date(timeIntervalSinceReferenceDate: 1_000)

    @Test("semantic state precedence is total across every active signal combination")
    func precedenceMatrix() {
        for mask in 0 ..< 32 {
            let listening = mask & 1 != 0
            let error = mask & 2 != 0
            let speaking = mask & 4 != 0
            let thinking = mask & 8 != 0
            let success = mask & 16 != 0
            let resolution = AvatarStateReducer.resolve(
                AvatarInputs(
                    isListening: listening,
                    hasError: error,
                    hasRecentTextDelta: speaking,
                    isThinking: thinking,
                    hasRecentSuccess: success,
                    lastSignalAt: now,
                    now: now
                )
            )
            let expected: AvatarState
            if listening {
                expected = .listening
            } else if error {
                expected = .error
            } else if speaking {
                expected = .speaking
            } else if thinking {
                expected = .thinking
            } else if success {
                expected = .success
            } else {
                expected = .idle
            }
            #expect(resolution.state == expected)
        }
    }

    @Test("terminal states dwell for four seconds before returning to idle")
    func terminalDwell() {
        for error in [false, true] {
            let withinDwell = AvatarStateReducer.resolve(
                AvatarInputs(
                    hasError: error,
                    hasRecentSuccess: !error,
                    lastSignalAt: now.addingTimeInterval(-3.9),
                    now: now
                )
            )
            #expect(withinDwell.state == (error ? .error : .success))

            let afterDwell = AvatarStateReducer.resolve(
                AvatarInputs(
                    hasError: error,
                    hasRecentSuccess: !error,
                    lastSignalAt: now.addingTimeInterval(-4.1),
                    now: now
                )
            )
            #expect(afterDwell.state == .idle)
        }
    }

    @Test("every stalled signal falls back to idle after thirty seconds")
    func stalledSignals() {
        let stale = now.addingTimeInterval(-30.1)
        for state in AvatarState.allCases where state != .idle {
            let resolution = AvatarStateReducer.resolve(
                AvatarInputs(
                    isListening: state == .listening,
                    hasError: state == .error,
                    hasRecentTextDelta: state == .speaking,
                    isThinking: state == .thinking,
                    hasRecentSuccess: state == .success,
                    lastSignalAt: stale,
                    now: now
                )
            )
            #expect(resolution.state == .idle)
        }
    }

    @Test("a throwing runtime is disabled without changing the semantic state machine")
    @MainActor
    func throwingRuntimeIsolated() {
        let runtime = ThrowingAvatarRuntime()
        let controller = AvatarController(runtime: runtime)

        #expect(!controller.isRuntimeEnabled)
        #expect(controller.state == .idle)
        #expect(controller.errors.count == 1)

        controller.setManualState(.thinking)
        #expect(controller.state == .thinking)
        #expect(!controller.isRuntimeEnabled)
    }

    @Test("recording remains listening until the push-to-talk signal ends")
    @MainActor
    func listeningSignalPersistsAcrossWatchdogRefreshes() {
        let controller = AvatarController()

        controller.setListening(true)
        controller.refresh()
        #expect(controller.state == .listening)

        controller.setListening(false)
        #expect(controller.state == .idle)
    }

    @Test("debug shape adapter represents each semantic state with normalized renderer values")
    @MainActor
    func debugShapeRuntime() throws {
        let runtime = DebugShapeAvatarRuntime()
        try runtime.load()
        #expect(runtime.isLoaded)

        for state in AvatarState.allCases {
            try runtime.setState(state)
            #expect(runtime.state == state)
        }
        try runtime.setLipSync(-1)
        #expect(runtime.lipSync == 0)
        try runtime.setLipSync(2)
        #expect(runtime.lipSync == 1)
    }

    @Test("lip-sync sources and the global switch can each be disabled")
    @MainActor
    func lipSyncSwitches() {
        let now = Date.now
        let microphone = MicrophoneRMSSource()
        microphone.accept(0.6, at: now)
        #expect(microphone.value(at: now) == 0.6)
        microphone.isEnabled = false
        #expect(microphone.value(at: now) == 0)

        let text = TextCadenceLipSyncSource()
        text.recordTextDelta(at: now.addingTimeInterval(-0.2))
        text.recordTextDelta(at: now)
        #expect(text.value(at: now) > 0)
        text.isEnabled = false
        #expect(text.value(at: now) == 0)

        let runtime = DebugShapeAvatarRuntime()
        let controller = AvatarController(runtime: runtime)
        controller.setManualState(.listening)
        controller.setMicrophoneRMS(0.75)
        #expect(runtime.lipSync == 0.75)
        controller.setLipSyncEnabled(false)
        #expect(runtime.lipSync == 0)
    }

    @Test("mapping envelope accepts a second adapter and invalid files fall back safely")
    func mappingEnvelope() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("vela-avatar-mapping-\(UUID().uuidString)", isDirectory: true)
        let mappingURL = directory.appendingPathComponent("mapping.json")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }

        let mapping = AvatarMappingConfiguration(
            stateExpressions: [.success: "celebrate"],
            stateMotions: [.thinking: "wait"],
            adapters: [
                "debug_shape": AvatarAdapterMapping(values: ["accent": "green"]),
                "hypothetical_renderer": AvatarAdapterMapping(values: ["signal": "awake"]),
            ]
        )
        try JSONEncoder().encode(mapping).write(to: mappingURL)
        let loaded = AvatarMappingLoader.load(from: mappingURL)
        #expect(loaded.mapping == mapping)
        #expect(loaded.diagnostic == nil)

        try Data("not json".utf8).write(to: mappingURL)
        let invalid = AvatarMappingLoader.load(from: mappingURL)
        #expect(invalid.mapping == .builtIn)
        #expect(invalid.diagnostic != nil)
    }

    @Test("visible renderers that stop producing frames transition through error then idle")
    @MainActor
    func renderWatchdog() {
        let clock = TestClock(date: Date(timeIntervalSinceReferenceDate: 1_000))
        let runtime = StalledAvatarRuntime(lastRenderedAt: clock.date.addingTimeInterval(-3))
        let controller = AvatarController(runtime: runtime, now: { clock.date })

        #expect(controller.state == .error)
        clock.date = clock.date.addingTimeInterval(AvatarStateReducer.terminalDwell + 0.1)
        controller.refresh()
        #expect(controller.state == .idle)

        runtime.lastRenderedAt = clock.date
        controller.refresh()
        #expect(controller.state == .idle)
    }
}

@MainActor
private final class TestClock {
    var date: Date

    init(date: Date) {
        self.date = date
    }
}

@MainActor
private final class ThrowingAvatarRuntime: AvatarRuntime {
    private enum Failure: LocalizedError {
        case load

        var errorDescription: String? { "intentional test failure" }
    }

    func load() throws { throw Failure.load }
    func unload() {}
    func setState(_: AvatarState) throws {}
    func setExpression(_: String?) throws {}
    func playMotion(_: String) throws {}
    func setLipSync(_: Double) throws {}
    func lookAt(x _: Double, y _: Double) throws {}
}

@MainActor
private final class StalledAvatarRuntime: AvatarRuntime {
    let isPresentationVisible = true
    var lastRenderedAt: Date?

    init(lastRenderedAt: Date?) {
        self.lastRenderedAt = lastRenderedAt
    }

    func load() throws {}
    func unload() {}
    func setState(_: AvatarState) throws {}
    func setExpression(_: String?) throws {}
    func playMotion(_: String) throws {}
    func setLipSync(_: Double) throws {}
    func lookAt(x _: Double, y _: Double) throws {}
}
