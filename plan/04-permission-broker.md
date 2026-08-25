# Phase 04 — Permission Broker

## Hypothesis

Vela can safely mediate agent requests for sensitive actions without coupling permission policy to a specific harness.

## Objectives

- Normalize ACP permission requests into Vela-owned permission intents.
- Surface requests in native macOS UI.
- Support allow-once, allow-for-session, and deny decisions.
- Record decisions and relevant provenance.
- Ensure unresolved permission requests cannot deadlock the entire runtime.

## Scope

Initial permission categories:

```text
filesystem.read
filesystem.write
shell.execute
network.open_url
mcp.invoke
other
```

Each request should include sufficient context for a user decision:

- requesting agent/session;
- action summary;
- target resource/command when available;
- requested scope;
- correlation ID.

## Non-goals

- Permanent workspace-scoped policies.
- Complex enterprise policy engines.
- Silent auto-approval based on model judgment.
- Provider-specific permission UI.

## Deliverable

A real or fake ACP session can pause on a permission request, display a native prompt, receive a decision, continue or reject the operation, and retain an auditable event record.

## Acceptance Criteria

- [ ] Permission requests are represented by Vela-owned domain types.
- [ ] Native UI shows enough detail to make a meaningful decision.
- [ ] `Allow once` applies only to the current request.
- [ ] `Allow for session` is limited to the documented session scope.
- [ ] `Deny` is propagated cleanly to the harness.
- [ ] Closing/dismissing the UI resolves according to an explicit safe default.
- [ ] Concurrent permission requests do not overwrite each other.
- [ ] Session cancellation resolves pending requests and cleans state.
- [ ] Decisions are written to structured events with session/request provenance.
- [ ] Tests cover allow, deny, session grant, timeout/dismissal, concurrent requests, and cancellation.

## Validation Procedure

1. Use the fake harness to request each permission category.
2. Exercise every decision path.
3. Trigger two concurrent permission requests.
4. Cancel a session while a prompt is open.
5. Verify event history and no stale grants leak into a new session.
6. Exercise a harmless real-harness permission request when supported.

## Evidence to Capture

- Permission state machine.
- Screenshots of native decision UI.
- Example audit event.
- Scope rules for session grants.

## Exit Gate — Execution Plane

After this phase, evaluate the first product gate:

> Can Vela reliably discover, control, observe, and safely mediate a local ACP-compatible agent runtime?

If the answer is not clearly yes, revise the execution plane before building workspace and presence features.
