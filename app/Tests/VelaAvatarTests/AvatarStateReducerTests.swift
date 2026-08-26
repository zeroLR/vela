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
