# Phase 07 — Avatar Presence

## Preconditions

Phase 06 records Gate B as **pending**: real-device hotkey/microphone interaction and
multiple normal work sessions of dogfood metrics are not yet captured, and
[`06-capture.md`](06-capture.md) states that Phase 07 must not begin until that evidence
exists. That constraint is retained here deliberately. Avatar presence is the first
milestone that is not on the work-utility critical path, so starting it before capture
utility is demonstrated inverts the ordering the plan exists to protect.

Stage 07a may begin once Gate B evidence is recorded, whatever its outcome — a Gate B
failure changes what the presence layer should reflect, not whether the abstraction is
needed.

## Hypothesis

A semantic avatar layer can improve the sense of presence and provide immediate runtime
feedback without coupling the assistant domain to any renderer's expression, motion, or
model parameters.

This decomposes into two independently falsifiable claims:

1. **Presentation** — continuous, non-textual feedback about assistant state is useful
   during real work. Testable with any renderer.
2. **Rendering** — a specific character renderer is worth its build, licensing, and
   resource cost relative to a simpler one.

The stage split below exists so claim 1 is answered before claim 2 is paid for. Rive
answers claim 1 interactively at low cost; Live2D is the intended answer to claim 2 and
is deferred to Stage 07d rather than assumed.

## Objectives

- Define `AvatarRuntime` as a Vela-owned abstraction.
- Derive semantic assistant state deterministically from signals Vela already publishes.
- Ship a working presence surface that does not depend on any vendor SDK.
- Validate presence interactively with a Rive character behind that abstraction.
- Keep the Live2D route reachable without changing the abstraction.
- Provide lip-sync input from sources that exist today.
- Keep avatar rendering isolated from ACP, capture, and workspace logic.

## Stage Split

| Stage | Goal | Vendor SDK | Primary risk removed |
|---|---|---|---|
| 07a | `AvatarRuntime` contract, semantic state machine, built-in renderer | No | State derivation, stuck states, failure isolation |
| 07b | Floating presence surface | No | Window behavior, focus stealing, resource cost |
| 07c | Rive character adapter | Rive (MIT) | Character rendering, mapping, interactive validation |
| 07d | Live2D route — deferred | Live2D Web | Licensing, WebView cost, model loading |

The ordering is chosen so that most acceptance criteria are provable without any
third-party renderer, and so that 07a + 07b remain a shippable presence layer if 07c
slips. Stage 07d is recorded, not scheduled: it is outside the Phase 07 exit decision and
is revisited after Gate C.

## Semantic States

Initial states:

```text
idle
listening
thinking
speaking
success
error
```

The renderer decides how a state becomes an expression/motion. The domain never names an
expression, motion, or model parameter.

### State derivation ownership

Semantic state is computed in Swift by a pure reducer over signals the application
already holds. No new IPC method, event, or Core responsibility is introduced.

| Input signal | Source | Resulting state |
|---|---|---|
| `SpeechCaptureController.state == .recording` | `app/Sources/VelaApp/SpeechCaptureController.swift` | `listening` |
| `session.state == .running`, `toolStarted` without matching `toolFinished` | `IPCClient.session`, `IPCClient.sessionEvents` | `thinking` |
| `pendingPermissions` non-empty | `IPCClient.pendingPermissions` | `thinking` |
| `textDelta` received within the streaming window | `IPCClient.sessionEvents` | `speaking` |
| `completed(stopReason:)` | `IPCClient.sessionEvents` | `success` |
| `failed(code:message:)`, `IPCClient.state == .degraded`, unexpected Core exit | `IPCClient` | `error` |
| none of the above | — | `idle` |

Precedence is fixed and total: `listening` > `error` > `speaking` > `thinking` >
`success` > `idle`. Every input combination therefore resolves to exactly one state, and
the reducer is exhaustively testable without a renderer, a window, or a model.

### Fallback timings

| Rule | Timeout | Result |
|---|---:|---|
| `success` / `error` dwell | 4s | return to `idle` |
| Any non-`idle` state with no new input | 30s | return to `idle` |
| Renderer reports no completed frame while visible | 2s | `error`, then `idle` |
| Adapter throws on any call | immediate | disable adapter, log, continue in `idle` |

## Architectural Decisions

### D1 — State derivation lives in Swift, not Core

The signals are already published by `IPCClient` and `SpeechCaptureController`. Core has
no knowledge of the microphone, push-to-talk, or window visibility, so moving derivation
into Core would require exporting presentation concerns into a domain contract that
exists to stay provider-neutral. Keeping the reducer in Swift also means an avatar defect
cannot reach Core, capture, or ACP.

### D2 — Lip-sync uses sources that exist today; TTS is not in this phase

Vela currently has speech **input** only (`SFSpeechRecognizer`); there is no
text-to-speech or audio playback layer, and `docs/TECH_STACK.md` still lists Speech as
adapter-based with no implementation. Microphone amplitude corresponds to the user
speaking, which is `listening`, not `speaking`.

Phase 07 therefore defines a `LipSyncSource` protocol with two implementations:

- **microphone RMS**, computed from the existing audio tap, used during `listening`;
- **text-delta cadence**, a deterministic function of streaming token arrival rate, used
  during `speaking`.

Audio-accurate lip sync driven by synthesized speech is deferred until a speech-output
adapter exists. This is a scope reduction, recorded as such, not an omission.

### D3 — Rive first for interactive validation; Live2D stays reachable

The app is a pure SwiftPM package (`app/Package.swift`). Cubism SDK for Native would
require a C++/CMake build, an Objective-C++ bridge, a closed-source core library inside
the signed bundle, and an `xcframework` path the app does not have — invalidating Phase
00's build assumptions and adding a Phase 09 signing dependency. It is out of scope.

The first character renderer is Rive (Stage 07c). It is MIT-licensed, installs through
SPM, supports macOS 13.1+, and has no licensing question blocking the start of work, so
the presence hypothesis can be validated interactively without first resolving a vendor
license.

Live2D remains the intended route and is recorded as Stage 07d, reached through
`WKWebView` rather than the native C++ SDK. Deferring it is not a rejection: it moves the
Live2D licensing and resource questions after the point where presence has been shown to
be worth having, so a negative answer costs nothing already built.

### D4 — Avatar configuration is device state, not workspace state

Model path, mapping file, enabled/disabled, and anchor position live under
`~/Library/Application Support/dev.vela.app/avatar/`. They describe a machine's
presentation, not the user's work, so they must not enter the workspace, its event
history, or its provenance model. Models are referenced by path and never copied into the
app bundle, which also keeps character asset licensing out of Vela's distribution.

### D5 — The Stage 07d WebView renders the avatar only

A web runtime introduced for avatar rendering must not accumulate Vela UI. The README
commits to native presence via SwiftUI/AppKit, and `docs/TECH_STACK.md` defers Electron
and Tauri as the main UI runtime; hosting application UI in a `WKWebView` is a step
toward the deferred option under a different name. The Stage 07d WebView exposes one
avatar command channel and nothing else. Moving UI into it is a separate architectural
decision requiring its own justification, and is not implied by choosing the Live2D
route.

### D6 — The contract has two intended consumers, so it must not learn Rive's vocabulary

Rive's state machine fits the semantic model so well that the abstraction could quietly
become a Rive interface: `setState` collapsing into "set the trigger named X",
`mapping.json` becoming an input-name table, lip sync becoming a Rive number input. Any
of those would have to be undone to reach Stage 07d, where the same states must drive
Live2D expression files, motion files, and `ParamMouthOpenY`.

The contract therefore carries semantic states and normalized values only. Renderer
vocabulary lives in the adapter and in an adapter-specific block of `avatar/mapping.json`
under a renderer-neutral envelope. A Live2D mapping must be an additional block, not a
schema change. Stage 07c treats "the protocol is unchanged from 07a" as an acceptance
criterion for exactly this reason.

## Non-goals

- Avatar marketplace.
- Advanced character authoring tools.
- LLM-controlled low-level model parameters.
- Multiple avatar formats in the first implementation.
- Full persona system.
- Text-to-speech or audio playback.
- Bundling any character asset with Vela.
- Cubism SDK for Native, or any vendor C++ SDK in the application build.
- Vela application UI rendered in a web runtime.

## Resource Budget

These are acceptance thresholds, not observations. Exceeding one is a revise-or-stop
decision recorded in this file, not a silently raised limit.

| Condition | CPU (Apple silicon, 60s average) | Memory (RSS delta over baseline) |
|---|---:|---:|
| Disabled or hidden | ≤ 0.1%, zero rendered frames | ≤ 1 MB |
| Visible, idle animation | ≤ 3% | ≤ 20 MB (07a/07b renderer) |
| Visible, state transitions and lip sync | ≤ 10% | ≤ 60 MB (07c, Rive character loaded) |
| Visible, Live2D in `WKWebView` | ≤ 10% | ≤ 250 MB (07d gate threshold) |

Hidden must mean *not rendering*. A hidden window that continues to submit frames fails
this budget regardless of its measured cost.

Measurement is per-application, not per-process. A `WKWebView` renderer's content and GPU
helper processes count toward these numbers, as does any third-party runtime's own
threads. A renderer that meets the budget only by excluding its helper processes has not
met the budget.

The 07d row is deliberately looser than 07c: a WebView carries process overhead a native
renderer does not, and that difference is part of what Stage 07d has to justify. It is a
gate threshold, not a target to grow into.

---

## Stage 07a — Runtime Contract and Semantic State Machine

### Scope

A new `VelaAvatar` Swift target containing:

- `AvatarRuntime` protocol matching [`docs/ARCHITECTURE.md`](../docs/ARCHITECTURE.md) §11:
  `load`, `unload`, `setState`, `setExpression`, `playMotion`, `setLipSync`, `lookAt`;
- `AvatarState` and `AvatarInputs` value types;
- `AvatarStateReducer`, a pure `(AvatarInputs) -> AvatarState` function;
- `AvatarController`, a `@MainActor` observer binding `IPCClient` and
  `SpeechCaptureController` to a runtime, including the watchdog timings above;
- `LipSyncSource` protocol with microphone-RMS and text-delta implementations, each
  independently disableable;
- mapping configuration loading from `avatar/mapping.json`, using a renderer-neutral
  envelope with an adapter-specific block (D6), and a built-in default used when the file
  is absent or invalid;
- `DebugShapeAvatarRuntime`, a SwiftUI renderer using shape/symbol/color per state — the
  first real adapter, not a stub;
- diagnostics: manual trigger for every semantic state, current state and last transition
  reason, and the avatar error log.

### Deliverable

Diagnostics can drive every semantic state manually and observe the built-in renderer
react, and a real capture or ACP session drives the same states automatically, with no
vendor SDK present in the repository.

### Acceptance Criteria

- [ ] Swift/UI code depends on `AvatarRuntime` only; no renderer type appears outside its adapter.
- [ ] The reducer is a pure function with exhaustive unit coverage over the input matrix.
- [ ] Every initial semantic state has a deterministic fallback reaction in the built-in renderer.
- [ ] Watchdog rules return the avatar to `idle` from `success`, `error`, and every stalled non-`idle` state.
- [ ] An adapter that throws on any call is disabled without affecting capture, workspace, or ACP.
- [ ] Lip-sync input can be disabled independently of avatar state, and each source can be disabled independently of the other.
- [ ] Expression/motion mappings are loaded from configuration; no mapping decision exists in agent prompt logic or in Core.
- [ ] The protocol and mapping envelope contain no renderer-specific vocabulary, verified by writing a second adapter block for a hypothetical renderer without changing the schema.
- [ ] Diagnostics expose avatar load, render, and state errors with the transition that caused them.
- [ ] `scripts/check.sh` remains green with the new target and tests.

### Validation Procedure

1. Trigger each semantic state manually from diagnostics.
2. Run a complete capture → agent response flow and observe transitions.
3. Trigger an ACP failure and verify recovery to the error/idle fallback.
4. Kill Core mid-session and verify `error` then `idle`, with capture still functional.
5. Inject a throwing adapter and verify the app continues with the avatar disabled.

---

## Stage 07b — Floating Presence Surface

### Scope

- Transparent, borderless, always-on-top, non-activating `NSPanel` that never takes key
  focus, reusing the collection behavior already proven by `QuickCapturePanel`
  (`.canJoinAllSpaces`, `.fullScreenAuxiliary`).
- Click-through by default (`ignoresMouseEvents`), with an explicit interaction mode.
- A hosting seam that accepts either a native view or a `WKWebView` without changing the
  window, so 07c's bake-off compares renderers rather than window implementations.
- Screen-corner anchoring, persisted per display.
- Show/hide, and a global enable/disable that unloads the runtime entirely.
- Render loop that stops when hidden, occluded, or on battery-saver conditions.
- Resource measurement against the budget table.

### Deliverable

An always-available presence surface that reflects Vela state during normal work without
stealing focus, following the user across spaces and full-screen applications, and
costing nothing measurable when disabled.

### Acceptance Criteria

- [ ] The panel never becomes key or main and never interrupts typing in another application.
- [ ] Clicks pass through to the application underneath in the default mode.
- [ ] The panel remains visible across space switches and over full-screen applications.
- [ ] Anchor position survives app restart and display reconfiguration.
- [ ] Disabling presence unloads the runtime; no frames are rendered and no timer runs.
- [ ] Hiding the panel stops rendering rather than only hiding the window.
- [ ] The window renders a transparent background with no border, shadow, or title bar artifacts over light and dark desktops.
- [ ] The hosting seam is demonstrated with both a native view and a placeholder `WKWebView`, proving Stage 07d needs no window changes.
- [ ] Measured CPU and memory meet the resource budget in every row of the table.
- [ ] Quick Capture (`⌥Space`) and push-to-talk behave identically with presence enabled and disabled.

### Validation Procedure

1. Work normally in an editor, terminal, and browser with the panel visible; confirm no
   focus or input interference.
2. Switch spaces and enter a full-screen application; confirm continued visibility.
3. Toggle presence off and sample CPU and rendered-frame counters.
4. Disconnect and reconnect a display; confirm anchor recovery.
5. Sample all three budget rows over 60s each and record the numbers.

---

## Stage 07c — Rive Character Adapter

### Renderer Decision — Recorded

Cubism SDK for Native is withdrawn (D3). The first character renderer is **Rive**, on a
transparent, borderless, always-on-top `NSWindow`. Live2D remains an intended route and
moves to Stage 07d, deferred.

Rive is chosen for interactive validation because its state machine is the closest
available match to Vela's semantic model: `setState` becomes a trigger or boolean input,
`setLipSync` becomes a number input driving a mouth blend, and iteration on the character
happens in the Rive editor rather than in Swift. `rive-app/rive-ios` is MIT-licensed and
ships an AppKit/SwiftUI runtime through SPM supporting macOS 13.1+, so no vendor binary
enters the C++ build or the notarization path, and no licensing question blocks the start
of interactive work.

That closeness is also the risk this stage must manage. See D6.

### Entry Gate

- [ ] The Rive editor plan required to author and export the character is identified,
      including whether a paid tier is needed for the intended use.
- [ ] The license of any character asset — authored, purchased, or community — permits
      the intended distribution.
- [ ] `rive-ios` builds and runs against the app's macOS 14 target through SPM.

The MIT license of the runtime and its macOS support are verified; the open items are
authoring and asset licensing, which concern the character rather than the runtime.

### Scope

- `RiveAvatarRuntime` implementing `AvatarRuntime`, hosted in the 07b window.
- Character loading from the configured `.riv` path, with the state-to-input mapping read
  from the adapter block of `avatar/mapping.json`.
- Idle animation, blink and look-at where the character supports them, state-driven
  reactions, and lip-sync parameter input.
- Automatic fallback to `DebugShapeAvatarRuntime` on load or render failure.
- Interactive validation: use the avatar during real work for multiple sessions and
  record whether the state feedback is read, ignored, or found distracting.

### Acceptance Criteria

- [ ] No Rive type is visible outside the adapter target; the Swift app compiles against `AvatarRuntime` unchanged from 07a.
- [ ] A configured `.riv` character loads from the configured location.
- [ ] A missing, corrupt, or incompatible character falls back to the built-in renderer, logs a diagnostic, and leaves capture, workspace, and ACP fully functional.
- [ ] Every semantic state maps to a character reaction through configuration, with a defined fallback when the character lacks the named input.
- [ ] Lip-sync is driven by `LipSyncSource` through a normalized value, not a Rive-named input, and remains independently disableable.
- [ ] Measured CPU and memory with the character loaded meet the resource budget.
- [ ] The shipped bundle passes the codesign and notarization checks required by Phase 09.
- [ ] `scripts/check.sh` remains green on a machine without the character asset present.
- [ ] The `AvatarRuntime` protocol is unchanged from 07a, or every change is justified as renderer-neutral in this file.

### Deliverable

A configured Rive character reflects Vela runtime state during capture and ACP sessions,
is used during real work for several sessions, and degrades to the built-in renderer on
every failure mode rather than to a broken app.

### Validation Procedure

1. Load the character and trigger each semantic state from diagnostics.
2. Run a complete capture → agent response flow and record the transitions.
3. Trigger an ACP failure and verify recovery to the error/idle fallback.
4. Break the character path intentionally and verify fallback plus continued Vela function.
5. Remove the character asset and verify the repository still builds and tests green.
6. Sample the budget rows with the character loaded and record the numbers.
7. Dogfood the presence surface during normal work and record the engagement observations
   that Gate C will need.

---

## Stage 07d — Live2D Route (Deferred)

Live2D is an intended route, not a Phase 07 deliverable. This stage exists so the target
stays recorded with its conditions, and so 07a–07c cannot quietly foreclose it. It is not
part of the Phase 07 exit decision and is revisited after Gate C.

### Shape

Transparent `WKWebView` in the 07b window hosting PixiJS with Cubism SDK for Web, behind
the same `AvatarRuntime`. The WebView renders the avatar only (D5). Cubism SDK for Native
stays out of scope: the Web route avoids the C++ build and the vendor-dylib signing
problem entirely.

### Gate

- [ ] The Live2D Publication License terms, the applicability of the small-scale
      exemption, and `live2dcubismcore.js` redistribution obligations are verified for a
      signed, notarized Developer ID app. Switching from Native to Web does not remove
      this requirement.
- [ ] The model asset license permits the intended distribution.
- [ ] `WKWebView` transparency is achieved on the target macOS version. The current
      approach depends on the undocumented `drawsBackground` KVC rather than public API;
      a public-API path or an accepted fallback must exist.
- [ ] A `WKWebView` renderer meets every row of the resource budget with its content and
      GPU helper processes included.
- [ ] Web content process termination is recovered or degraded to the built-in renderer
      without affecting capture, workspace, or ACP.

### What 07a–07c Must Preserve

- `AvatarRuntime` carries semantic states and normalized values only, never renderer
  vocabulary (D6).
- The 07b window hosts either a native view or a `WKWebView` without modification.
- `avatar/mapping.json` keeps a renderer-neutral envelope with an adapter-specific block,
  so a Live2D mapping is an additional block rather than a schema change.
- Adapter selection is configuration, so both adapters can coexist in one build.

---

## Criteria Traceability

The original Phase 07 criteria are preserved, each assigned to the stage that can prove it.

| Original criterion | Stage |
|---|---|
| UI depends on `AvatarRuntime`, not renderer types | 07a, re-verified in 07c |
| Character loads from a configured location | 07c |
| Every semantic state has a deterministic fallback | 07a, extended in 07c |
| Transitions cannot leave the avatar stuck | 07a |
| Avatar failure does not prevent capture/workspace/ACP | 07a, 07b, 07c |
| Lip-sync can be disabled independently | 07a |
| Mappings are configuration, not agent prompt logic | 07a, 07c |
| The contract stays renderer-neutral for a second adapter | 07a, 07b, 07c |
| Diagnostics expose load/render/state errors | 07a, 07c |

## Evidence to Capture

- The state input matrix and its resolved outputs, as generated by the reducer tests.
- The `mapping.json` envelope plus its Rive adapter block, with a second hypothetical
  block showing a new renderer needs no schema change.
- A short state-transition recording or screenshot sequence covering all six states.
- Failure fallback behavior: throwing adapter, missing character, web-process-style
  termination, Core exit.
- Measured CPU and memory for every applicable row of the resource budget, per stage.
- Focus and click-through behavior during real work in another application, plus the
  hosting-seam proof with a placeholder `WKWebView`.
- Interactive validation notes: whether the state feedback was read, ignored, or found
  distracting during real work — the input Gate C needs.

## Open Decisions

These change scope and are recorded rather than assumed silently.

1. **Gate B ordering** — record the Phase 06 dogfood evidence first, or accept a
   documented exception. This file assumes the former.
2. **Renderer sequencing** — *resolved.* Live2D is an intended route; Rive comes first
   for interactive validation. Recorded as D3, implemented as Stage 07c, with Live2D
   preserved as Stage 07d. The open part is only *when* 07d is attempted, which Gate C
   informs.
3. **Lip-sync scope** — this file assumes D2: no TTS in Phase 07. Reversing that pulls a
   speech-output adapter into this phase.

## Exit Decision

Proceed only if the avatar provides useful presence/status feedback without destabilizing
the work utility path or consuming disproportionate resources.

Stage-level outcomes:

- **07a fails** — the semantic state model is wrong. Fix it before any renderer work; no
  renderer rescues an incorrect or stuck state machine.
- **07b fails on resources or focus behavior** — presence is not viable as an
  always-visible surface. Consider state feedback inside existing surfaces (menu bar,
  Quick Capture panel) instead.
- **07c is blocked or disproportionate** — ship 07a + 07b, record the reason, and proceed
  to Phase 08. Presence is delivered; only the character is deferred.
- **07d** is outside this decision. It is attempted only if presence proves worthwhile at
  Gate C and its licensing gate passes. If the abstraction has drifted such that 07d would
  require changing `AvatarRuntime`, that is a defect in 07c, not a reason to skip 07d.

Gate C ([`08-scheduled-assistance.md`](08-scheduled-assistance.md)) evaluates whether
presence and scheduled assistance increase useful engagement without becoming
distracting. A presence layer that cannot be disabled, or that is not measured against
the budget above, cannot be evaluated by that gate.
