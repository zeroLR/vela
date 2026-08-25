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

- [x] Permission requests are represented by Vela-owned domain types.
- [x] Native UI shows enough detail to make a meaningful decision.
- [x] `Allow once` applies only to the current request.
- [x] `Allow for session` is limited to the documented session scope.
- [x] `Deny` is propagated cleanly to the harness.
- [x] Closing/dismissing the UI resolves according to an explicit safe default.
- [x] Concurrent permission requests do not overwrite each other.
- [x] Session cancellation resolves pending requests and cleans state.
- [x] Decisions are written to structured events with session/request provenance.
- [x] Tests cover allow, deny, session grant, timeout/dismissal, concurrent requests, and cancellation.

## Validation Procedure

1. Use the fake harness to request each permission category.
2. Exercise every decision path.
3. Trigger two concurrent permission requests.
4. Cancel a session while a prompt is open.
5. Verify event history and no stale grants leak into a new session.
6. Exercise a harmless real-harness permission request when supported.

The 2026-08-25 real-adapter attempt used `codex-acp` 1.6.2 and `claude-agent-acp` 0.70.0. Both inherited provider policies executed a harmless `/private/tmp` file tool call without emitting ACP `session/request_permission`. Codex was also retried with test-only `agent`, `on-request`, and `workspace-write` settings, but `/private/tmp` remained an allowed root and produced no request. An outside-workspace write was not attempted because a missing callback could mutate user files. Real permission mediation therefore remains unverified rather than counted as passing evidence. All zero-byte temporary files were removed. Deterministic fake-harness and Swift↔Rust cross-runtime paths cover the complete broker lifecycle.

## Evidence to Capture

- Permission state machine.
- Screenshots of native decision UI.
- Example audit event.
- Scope rules for session grants.

Implementation evidence is recorded in [`../docs/PERMISSION_BROKER.md`](../docs/PERMISSION_BROKER.md), [`../schemas/ipc-v1.md`](../schemas/ipc-v1.md), and the Rust/Swift integration suites.

## Exit Gate — Execution Plane

After this phase, evaluate the first product gate:

> Can Vela reliably discover, control, observe, and safely mediate a local ACP-compatible agent runtime?

If the answer is not clearly yes, revise the execution plane before building workspace and presence features.

Gate result on 2026-08-25: **revise**. Vela reliably mediates adapters that emit ACP permission requests, but the installed real-adapter configurations did not emit one for the safe test. Do not start Phase 05 until a disposable sandbox can prove a real provider request cannot bypass the broker.

Phase 04.1 supersedes this provisional result. Vela now pins safe ACP session modes before readiness, and disposable real write probes for both installed adapters reached the broker, were denied, were audited, and left no target file. Gate A is **pass**; see [`04.1-real-adapter-enforcement.md`](04.1-real-adapter-enforcement.md).
