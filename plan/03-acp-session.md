# Phase 03 — ACP Session Runtime

## Hypothesis

Vela can run a complete ACP session lifecycle through a normalized runtime without leaking ACP/provider-specific semantics into the UI.

## Objectives

- Start and stop ACP agent processes through the process manager.
- Create a session and submit prompts.
- Normalize streaming updates into Vela `AgentEvent` values.
- Support cancellation, terminal states, and process failure.
- Add a fake ACP harness capable of deterministic protocol scenarios.

## Scope

Minimum normalized lifecycle:

```text
idle
→ starting
→ ready
→ running
→ completed | cancelled | failed
```

Minimum normalized events:

```text
TextDelta
PlanUpdated
ToolStarted
ToolFinished
PermissionRequested
UsageUpdated
Completed
Failed
```

The fake ACP harness should support scripted scenarios for normal streaming, delayed responses, cancellation, permission requests, unexpected harness exit, malformed events, and optional resume capability.

## Non-goals

- Permission UI behavior beyond surfacing a request.
- Workspace mutation.
- Multi-agent orchestration.
- Provider-specific session features that cannot be normalized cleanly.

## Deliverable

From the macOS app, a user can select a discovered agent, create a session, submit text, see streamed output, cancel execution, and recover from an unexpected harness exit. The same scenarios run deterministically against a fake ACP harness in CI.

## Acceptance Criteria

- [ ] Session creation works through a Vela-owned API.
- [ ] UI renders streamed text without using ACP wire types.
- [ ] Exactly one terminal event is accepted per run.
- [ ] Cancellation propagates to the harness and terminates local run state.
- [ ] Unexpected agent-process exit mid-run produces a normalized failure and releases resources.
- [ ] New sessions can be created after failure without restarting Vela Core.
- [ ] Unsupported capabilities are handled explicitly rather than silently assumed.
- [ ] Session IDs, process IDs, and request correlation IDs are present in diagnostics.
- [ ] Fake harness integration tests cover success, cancel, unexpected exit, malformed event, and timeout.
- [ ] At least one real Claude or Codex ACP path is manually verified if available locally.

## Validation Procedure

1. Run all scripted fake-harness scenarios.
2. Verify event ordering and terminal-state invariants.
3. Simulate harness termination during streaming.
4. Restart a fresh session.
5. Use a real detected harness to submit a harmless read-only prompt.
6. Compare real events against normalized domain events and note unsupported/extra fields.

## Evidence to Capture

- Session lifecycle diagram after implementation.
- Example normalized event trace.
- Fake harness scenario catalog.
- Real harness compatibility notes and versions.

## Exit Gate — Execution Plane Part 1

Do not proceed to workspace/product features until real and fake ACP sessions are sufficiently stable that failures are observable and recoverable.
