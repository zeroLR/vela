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

- [ ] Global hotkey opens capture reliably while another app has focus.
- [ ] Text capture can be completed without opening the full workspace window.
- [ ] Push-to-talk clearly indicates recording state.
- [x] STT failure does not lose recorded/captured intent where recovery is possible.
- [x] Raw input and structured result are both retained.
- [x] User can correct routing/type after capture.
- [x] Capture path does not require starting a full ACP agent session unless explicitly needed.
- [x] Captures update workspace/event history with provenance.
- [ ] Current-state queries can answer active focus, blockers, and next actions from workspace state.
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

## Exit Gate — Work Utility

Evaluate:

> Does Vela materially reduce capture friction and maintain a useful representation of current work state?

**Pending.** The implementation slice and automated gate pass, but Gate B requires real-device interaction plus multiple normal work sessions of dogfood metrics. Do not proceed to Phase 07 until that evidence is recorded. If daily use does not naturally emerge, refine capture/state semantics before investing in avatar polish or proactive behaviors.
