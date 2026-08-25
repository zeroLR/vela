# Phase 02 — ACP Harness Discovery

## Hypothesis

Vela can reliably discover locally installed Claude/Codex tooling, match it to compatible ACP harness definitions, and expose normalized capabilities without owning provider authentication.

## Objectives

- Detect supported CLIs from PATH and known macOS install locations.
- Probe executable version safely.
- Match installed tooling to a harness launch specification.
- Initialize ACP and normalize advertised capabilities.
- Represent unavailable, unauthenticated, incompatible, and failed states distinctly.

## Scope

Discovery pipeline:

```text
candidate path
→ executable validation
→ version probe
→ harness match
→ ACP process launch
→ ACP initialize
→ capability normalization
→ AgentDescriptor registry
```

Initial targets:

- Claude-compatible ACP harness.
- Codex-compatible ACP harness.
- User-defined ACP harness entry for test coverage.

## Non-goals

- Sending real prompts.
- Reading private Claude/Codex credential files.
- Provider API keys managed directly by Vela.
- Automatic installation of CLIs/harnesses.

## Deliverable

A diagnostics/settings screen lists discovered agents with executable path, detected version, harness adapter, readiness state, and normalized capabilities.

## Acceptance Criteria

- [ ] Discovery finds supported tools installed via common macOS paths and PATH.
- [ ] Missing executables produce `unavailable`, not an exception loop.
- [ ] Version probing has a timeout and captures stderr/stdout diagnostics.
- [ ] ACP initialization has a timeout and process cleanup on failure.
- [ ] Capability output is represented with Vela-owned types.
- [ ] CLI credentials are never parsed from private provider config stores.
- [ ] Multiple agents can coexist in the registry.
- [ ] User-defined ACP harness config can register a fake harness.
- [ ] Discovery results can be refreshed without restarting Vela.
- [ ] Unit/integration tests cover present, missing, incompatible, timeout, and invalid harness cases.

## Validation Procedure

1. Test with neither supported CLI present using controlled PATH.
2. Add fake executables at known paths and verify detection precedence.
3. Test actual local Claude/Codex installs when available.
4. Force version-probe timeout.
5. Launch the fake ACP harness and verify capability normalization.
6. Corrupt one harness command and verify clean diagnostics/recovery.

## Evidence to Capture

- Detected executable paths and versions.
- Normalized `AgentDescriptor` examples.
- ACP initialize trace from fake harness.
- Failure-state screenshots/logs.

## Exit Decision

Proceed when agent discovery is deterministic enough that the UI never needs provider-specific detection logic.
