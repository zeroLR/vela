# Phase 07 — Avatar Presence

## Hypothesis

A semantic avatar layer can improve the sense of presence and provide immediate runtime feedback without coupling the assistant domain to Live2D-specific parameters.

## Objectives

- Define `AvatarRuntime` as a Vela-owned abstraction.
- Integrate the first Live2D Cubism adapter.
- Map normalized assistant/agent states to expressions and motions.
- Add basic lip-sync input from audio amplitude or speech playback.
- Keep avatar rendering isolated from ACP and workspace logic.

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

Agent/runtime events map to these semantic states. The model renderer decides how a state becomes an expression/motion.

## Scope

Initial adapter capabilities:

- load/unload model;
- idle animation;
- blink/look behavior where available;
- expression switching;
- motion playback;
- lip-sync parameter input;
- semantic state transitions.

## Non-goals

- Avatar marketplace.
- Advanced character authoring tools.
- LLM-controlled low-level model parameters.
- Multiple avatar formats in the first implementation.
- Full persona system.

## Deliverable

A floating/native avatar reflects Vela runtime state during capture and ACP sessions, including listening, thinking, speaking, success, and error transitions.

## Acceptance Criteria

- [ ] Swift/UI code depends on `AvatarRuntime`, not Cubism types outside the adapter.
- [ ] A supported model can be loaded from a configured location.
- [ ] Every initial semantic state has a deterministic fallback reaction.
- [ ] State transitions cannot leave the avatar permanently stuck after session completion/failure.
- [ ] Avatar failure does not prevent capture, workspace, or ACP functionality.
- [ ] Lip-sync input can be disabled independently.
- [ ] Model-specific expression/motion mappings are configuration, not agent prompt logic.
- [ ] Diagnostics expose avatar load/render/state errors.

## Validation Procedure

1. Load a known-good test model.
2. Trigger each semantic state manually from diagnostics.
3. Run a complete capture → agent response flow and observe transitions.
4. Trigger ACP failure and verify recovery to idle/error fallback.
5. Break the model path intentionally and verify Vela remains functional.

## Evidence to Capture

- State-to-expression/motion mapping example.
- Short state-transition recording or screenshots.
- Failure fallback behavior.
- CPU/memory observations while idle and animating.

## Exit Decision

Proceed only if the avatar provides useful presence/status feedback without destabilizing the work utility path or consuming disproportionate resources.
