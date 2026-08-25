# Phase 00 — Foundation

## Hypothesis

A minimal Swift/Rust monorepo can be built, tested, and diagnosed locally without prematurely committing to unstable application structure.

## Objectives

- Establish the repository boundaries described in `docs/TECH_STACK.md`.
- Create a native macOS app target and a Rust core executable.
- Establish shared schema ownership and fixture conventions.
- Establish structured logging before ACP/process complexity is introduced.
- Add CI for deterministic build/test checks.

## Scope

### macOS

- SwiftUI macOS application target.
- Minimal window or menu-bar-capable shell.
- `AppEnvironment`/dependency boundary for future IPC, audio, avatar, and notifications.
- Unit-test target.

### Rust

- Cargo workspace.
- `vela-core` executable.
- `domain` crate containing only stable Vela-owned identifiers and lifecycle primitives needed by the initial slice.
- `tracing`-based structured logging.
- Unit/integration test setup.

### Repository

Create concrete versions of:

```text
app/
core/
native/
schemas/
fixtures/
docs/
plan/
scripts/
```

Add formatting/linting commands that can run locally and in CI.

## Non-goals

- ACP integration.
- Unix socket protocol beyond a placeholder/version declaration.
- Live2D SDK integration.
- Workspace model.
- Signing/notarization.
- Provider or CLI detection.

## Deliverable

Running the macOS app and `vela-core` independently produces a visible/observable healthy startup. CI builds and tests both sides.

## Acceptance Criteria

- [ ] A clean checkout can build the macOS target with documented commands.
- [ ] `cargo build --workspace` succeeds.
- [ ] `cargo test --workspace` succeeds.
- [ ] Swift tests succeed from command line.
- [ ] Rust logs are structured and include timestamp, level, component, and process version.
- [ ] The app has a diagnostics/version view or equivalent debug surface showing app build information.
- [ ] CI runs Swift and Rust checks independently so failures are attributable to one runtime.
- [ ] No direct Anthropic/OpenAI SDK dependency exists.
- [ ] No ACP wire types exist outside a future adapter boundary.

## Validation Procedure

1. Clone into a new directory.
2. Run the documented bootstrap/build steps.
3. Launch the app.
4. Run `vela-core` from Terminal.
5. Intentionally trigger one test failure on each side to verify CI separation, then revert it.

## Evidence to Capture

- CI run URL or screenshot reference.
- Exact local bootstrap command.
- Swift/Xcode version used.
- Rust toolchain version used.
- Any environment assumptions discovered during setup.

## Exit Decision

Proceed only if both runtimes can be built and tested reproducibly. Fix tooling friction now; every later milestone depends on fast local iteration.
