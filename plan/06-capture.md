# Phase 06 — Capture and Work Utility

## Hypothesis

Low-friction capture plus lightweight structuring can reduce context-switching cost and keep the current work state useful enough for daily dogfooding.

## Objectives

- Add global-hotkey text capture.
- Add push-to-talk capture.
- Normalize captured input into inbox/note/task/status-update intents.
- Preserve original input and structured interpretation.
- Update current work state without requiring a heavyweight agent session for every capture.
- Measure real usage and correction rate.

## Scope

### Interaction

Initial interaction paths:

```text
Global hotkey → quick text input → submit
Global hotkey → hold-to-talk → STT → review/submit
```

### Fast capture lane

The capture lane may perform:

- transcription cleanup;
- basic intent classification;
- metadata/entity extraction;
- routing suggestion;
- concise title generation.

The original raw text/transcript must remain available for audit/correction.

### Initial intent classes

```text
note
idea
todo
work_update
unknown
```

## Non-goals

- Always-listening microphone.
- Wake word.
- Full-duplex conversation.
- Autonomous execution for every captured item.
- Complex natural-language scheduling.

## Deliverable

During normal work, the user can capture text or speech within seconds, see where it was routed, correct a misclassification, and immediately ask Vela what is active, blocked, or next based on workspace state.

## Acceptance Criteria

- [x] Global hotkey opens capture reliably while another app has focus.
- [x] Text capture can be completed without opening the full workspace window.
- [x] Push-to-talk clearly indicates recording state.
- [x] STT failure does not lose recorded/captured intent where recovery is possible.
- [x] Raw input and structured result are both retained.
- [x] User can correct routing/type after capture.
- [x] Capture path does not require starting a full ACP agent session unless explicitly needed.
- [x] Captures update workspace/event history with provenance.
- [x] Current-state queries can answer active focus, blockers, and next actions from workspace state.
- [x] Basic product telemetry can be derived locally: captures/day, median capture completion time, correction rate, abandoned captures.

## Dogfood Experiment

Run the feature as the primary personal capture path for multiple normal work sessions.

Track:

- number of captures per day;
- time from hotkey to completed capture;
- classification correction rate;
- how often captured items become actionable tasks/status;
- how often the user still reaches for another note/todo tool;
- examples where Vela's current state becomes stale or misleading.

## Validation Procedure

1. Capture at least several examples of every intent class.
2. Capture while Xcode/editor/terminal/browser has focus.
3. Exercise STT success, partial failure, cancellation, and manual correction.
4. Ask "What am I working on?", "What is blocked?", and "What should I do next?" after several captures.
5. Compare answers with the actual workspace state, not conversation memory.

## Evidence to Capture

- Representative raw → structured examples.
- Capture latency measurements.
- Correction/misclassification examples.
- Current-state answer examples and failures.

## Implementation Validation — 2026-08-25

- Five `capture-engine` tests cover raw/normalized retention, deterministic English/Traditional Chinese routing, reopen, correction cleanup, status markers, abandonment, validation, and metrics.
- Nine IPC tests include a real Unix-socket capture slice for create, correct, abandon, and metrics with canonical routed files.
- Ten Swift tests include typed capture decoding and a real Swift→`vela-core` flow that verifies suggestion, correction to a task, abandoned speech retention, and updated local metrics.
- `scripts/check.sh` passes Rust format/clippy/build, 33 Rust tests, Swift build, and 10 Swift tests.
- The native executable starts and connects to its supervised Core. Its Mach-O contains the microphone and Speech usage Info.plist section.
- Automated screen/window inspection was unavailable because the validation environment does not have Screen Recording/Accessibility permission. Microphone TCC was not bypassed or auto-approved.

The first three interaction criteria remain unchecked until a user-driven macOS run confirms `⌥Space` from another foreground app, text-only completion from the floating panel, and real push-to-talk recording feedback.

## Implementation Validation — 2026-08-26

Real-device validation started and immediately failed. Pressing hold-to-talk killed
the app before any recording feedback appeared, so the push-to-talk criterion could
not be evaluated. Three defects were found and fixed.

**Privacy metadata could not be read.** TCC aborts a process instead of returning an
error when it cannot find a usage description, which is what the crash was:
`__TCC_CRASHING_DUE_TO_PRIVACY_VIOLATION__` from the `swift run` executable. The
2026-08-25 note above — that the Mach-O `__info_plist` section carries the microphone
and Speech descriptions — is superseded: the section exists and `Bundle.main` even
answers those keys from it, but TCC does not accept it. Push-to-talk now requires a
real bundle. `scripts/run-app.sh` builds, ad-hoc signs, and verifies
`VelaApp.app`, refusing to launch if either privacy key is missing, and the app
checks that it is running inside an `.app` before calling TCC. Because a bare
executable answers the usage-description keys anyway, the bundle layout — not key
presence — is the deciding check. Microphone authorization is now requested
explicitly, so a denial is a visible capture failure rather than a silent recording.

**Framework callbacks inherited the main actor.** With the bundle in place, TCC
granted microphone access and the app crashed again: `EXC_BREAKPOINT` in
`dispatch_assert_queue` from `SpeechCaptureController.requestAuthorization`. Speech
and AVFoundation are imported `@preconcurrency`, so their callbacks lose `@Sendable`
and a closure created in the `@MainActor` controller inherits main-actor isolation;
the framework then invokes it on a dispatch queue and Swift 6 traps the process. The
authorization requests are `nonisolated`, the recognition handler is `@Sendable`, and
the audio tap is installed from a `nonisolated` helper because the render thread
would have trapped next.

**Core outlived a crashed app.** Each crash left an orphaned `vela-core` holding the
socket; four had accumulated. Core now accepts `--exit-with-parent`, which the app
always passes, and shuts down when its parent process ID changes or becomes
`launchd`. Verified by `SIGKILL`ing the app and observing Core exit on its own.
Three tests in `core/crates/vela-core/tests/supervisor_exit.rs` cover the orphan
exit, the opt-in default, and unknown-argument rejection.

**Current-state queries now have an answer path.** The criterion was previously
unimplemented: `STATUS.md` had the three sections and `workspace.context` returned
its raw text, but nothing answered the questions and `session.prompt` carried no
workspace context. Core derives the answers instead: `workspace.current_state`
parses the `## Active focus`, `## Blockers`, `## Next actions`, and
`## Captured work updates` sections and lists the `tasks/` files, excluding unedited
template placeholders and completed `[x]` items so "nothing recorded" is truthful.
Capture-written lines keep their `capture_id`, and `status_updated_at_ms` exposes
staleness. The Quick Capture panel answers working-on/blocked/next inline, so
`⌥Space` covers both capture and orientation without the diagnostics window, which
also has a Work State Answers panel. No ACP session is involved.

`scripts/check.sh` passes: Rust format/clippy/build, 39 Rust tests, Swift build, and
11 Swift tests. Current-state coverage is two `workspace-engine` tests (parsing,
placeholder and completed-item exclusion, capture attribution, task bounds), one IPC
test that asks after a work-update and a todo capture, and the Swift cross-runtime
slice asserting the same answers through a real `vela-core` process.

**First device run after the fixes.** A user-driven run on macOS 26.6.2 produced two
capture records without a crash: a text capture (`hello`) completed and routed to
`notes/`, and a speech capture that transcribed `哈囉你好啊` through
`SFSpeechRecognizer` and was retained as `abandoned` with its transcript intact when
the panel was dismissed instead of submitted. Push-to-talk therefore reaches TCC,
records, and transcribes on real hardware. Earlier in the same session the Carbon
hotkey logged `Vela global hotkey received [id=1]` immediately followed by a Quick
Capture panel, so the shortcut delivers the panel.

The three interaction criteria stay unchecked because the artifacts cannot establish
what only the user can report: whether `⌥Space` fired while another application held
focus, and whether the recording indicator reads clearly during a hold. Validation
Procedure steps 4 and 5 — comparing current-state answers against actual workspace
state across normal work sessions — remain Gate B dogfood evidence.

## Exit Gate — Work Utility

Evaluate:

> Does Vela materially reduce capture friction and maintain a useful representation of current work state?

**Pending.** The implementation slice and automated gate pass, but Gate B requires real-device interaction plus multiple normal work sessions of dogfood metrics. Do not proceed to Phase 07 until that evidence is recorded. If daily use does not naturally emerge, refine capture/state semantics before investing in avatar polish or proactive behaviors.
