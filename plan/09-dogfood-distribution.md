# Phase 09 — Dogfood Hardening and Distribution

## Hypothesis

Vela can be made reliable enough for continuous personal use and direct macOS distribution without compromising the ACP-first/local-first architecture.

## Objectives

- Harden diagnostics and failure recovery from real daily use.
- Define compatibility checks for Vela Core, ACP harnesses, and detected CLIs.
- Package Swift app + Rust core correctly.
- Prepare Developer ID signing, Hardened Runtime, and notarization.
- Add an application update path.
- Turn real dogfood failures into regression fixtures/tests.

## Scope

### Reliability

- launch/relaunch behavior;
- Core/harness crash recovery;
- stale session cleanup;
- workspace/index recovery;
- log retention/export;
- compatibility diagnostics;
- graceful behavior when Claude/Codex tooling changes.

### Distribution

- signed app bundle;
- bundled Vela Core executable;
- Hardened Runtime configuration;
- notarization workflow;
- update feed (for example Sparkle 2);
- stable/beta channel decision when needed.

### Diagnostics

Provide a user-accessible diagnostics surface for:

```text
App version
Core version
IPC protocol version
Detected agents and versions
ACP adapter/harness status
Workspace health
Recent runtime errors
Export diagnostics
```

## Non-goals

- Mac App Store distribution.
- Cloud accounts/sync.
- Team administration.
- Plugin marketplace.
- Production-scale analytics backend.

## Deliverable

A notarizable directly distributable Vela build can be installed on another Mac, discover supported local agent tooling, open/create a workspace, run the established MVP flows, and export enough diagnostics to investigate compatibility failures.

## Acceptance Criteria

- [ ] App bundle contains the expected Core executable and can launch it after installation.
- [ ] Fresh-install and upgrade paths are tested on a clean macOS user environment where practical.
- [ ] Missing Claude/Codex tooling yields actionable status instead of application failure.
- [ ] Known incompatible harness/CLI versions are reported clearly.
- [ ] Core and agent unexpected exits recover according to documented lifecycle rules.
- [ ] Workspace canonical data survives derived-index corruption/rebuild scenarios.
- [ ] Diagnostics can be exported without exposing secrets by default.
- [ ] Signing/notarization workflow is documented and reproducible once credentials are available.
- [ ] Update mechanism can verify and apply a test release.
- [ ] Dogfood defects that affected runtime correctness have regression tests or fixtures.

## Dogfood Checklist

Use Vela as the primary work companion long enough to exercise:

- daily capture;
- current-state queries;
- ACP discussion/execution;
- permissions;
- referenced folders;
- avatar presence;
- scheduled reports;
- app/core updates;
- agent CLI upgrades.

Record defects by subsystem and distinguish product-friction failures from runtime correctness failures.

## MVP Review

At the end of this phase, evaluate the product on outcomes rather than feature count:

### Utility

- Is capture genuinely faster than existing habits?
- Is current work state accurate enough to trust?
- Does Vela surface useful next actions?

### Agent control plane

- Are Claude/Codex/ACP sessions reliable enough for daily use?
- Are permission and failure states understandable?
- Can harness changes be diagnosed without changing the UI architecture?

### Presence

- Does the avatar improve awareness/engagement?
- Are scheduled reports useful rather than noisy?

### Architecture

- Can workspace data be inspected without Vela?
- Can SQLite be rebuilt from canonical state?
- Are external providers/harnesses replaceable behind adapters?
- Do logs/events provide enough evidence to debug agent behavior?

## Exit Decision

If the core utility is validated, move into post-MVP capabilities such as richer Skills, MCP configuration UX, personas/skins, configuration import/export, more avatar formats, and carefully scoped subagent orchestration.

If the utility is weak, preserve the validated execution/workspace infrastructure but revise the product interaction model before expanding platform features.
