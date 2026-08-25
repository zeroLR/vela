# Vela

Vela is a local-first macOS work companion designed to stay present, understand the current state of your work, and help move it forward.

The project combines a native desktop experience, a persistent workspace, voice/text capture, ACP-compatible agent harnesses, and an expressive avatar layer. The goal is not to build another chat client or coding-agent GUI. Vela acts as a personal agent control plane: it captures fragmented thoughts, maintains work context, surfaces the next useful action, and delegates execution to existing agent runtimes such as Claude Code and Codex.

## Core Principles

- **Local-first workspace** — durable work state lives on the local filesystem; SQLite is used for indexes, events, and caches.
- **ACP-first execution** — Vela discovers compatible local agent harnesses and interacts with them through ACP instead of reimplementing provider-specific agent runtimes.
- **Progressive context disclosure** — agents load the smallest useful amount of context first, then expand into plans, evidence, and referenced folders only when needed.
- **Observable and reversible** — agent activity, workspace mutations, permissions, and derived state should have clear provenance and event history.
- **Native presence** — SwiftUI/AppKit, voice interaction, notifications, and an avatar runtime make the assistant continuously accessible without becoming intrusive.
- **Replaceable boundaries** — ACP, avatar, speech, storage, and external integrations are adapters around stable Vela domain contracts.

## Proposed Architecture

```text
macOS App (SwiftUI / AppKit)
        |
        | local IPC
        v
Vela Core (Rust / Tokio)
        |
        +-- ACP Runtime --> Claude / Codex / other ACP agents
        +-- Workspace Engine --> Filesystem + SQLite
        +-- Event Store / Scheduler / Permission Broker

Native macOS layers:
- Audio: AVFoundation
- Avatar: pluggable AvatarRuntime, initially Live2D Cubism
- Secrets: Keychain
- Notifications: UserNotifications
```

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/TECH_STACK.md`](docs/TECH_STACK.md) for the working design.

## Roadmap

Vela is intentionally built as a sequence of independently testable vertical slices.

1. **Foundation** — establish the Swift/Rust monorepo, contracts, test fixtures, and diagnostics.
2. **Core IPC** — prove reliable bidirectional streaming between the macOS app and Rust core.
3. **ACP Harness Discovery** — detect Claude/Codex tooling and expose normalized capabilities.
4. **ACP Sessions** — start, stream, cancel, resume, and recover agent sessions end to end.
5. **Permission Broker** — mediate agent tool/file/shell permissions through native UI.
6. **Workspace** — introduce local-first work state, filesystem references, event provenance, and indexing.
7. **Capture** — add low-friction text and push-to-talk capture with structured inbox/task/note routing.
8. **Avatar Presence** — connect normalized assistant states to avatar expressions, motions, and lip sync.
9. **Scheduled Assistance** — add scheduled summaries and proactive work-state reports.
10. **Dogfood & Distribution** — harden observability, compatibility, notarization, updates, and daily-use flows.

The detailed, acceptance-test-driven execution plan lives in [`plan/`](plan/README.md).

## Development

Phase 00–03 establish the Swift/Rust skeleton, local IPC, ACP harness discovery, and the first complete ACP session runtime. The debug app can select a ready adapter, create a process/session, submit text, render normalized streaming events, cancel a run, and recover by creating a new session after adapter failure.

```bash
cargo build --manifest-path core/Cargo.toml --workspace
scripts/check.sh
VELA_CORE_PATH="$PWD/core/target/debug/vela-core" swift run --package-path app VelaApp
```

See [`schemas/ipc-v1.md`](schemas/ipc-v1.md) for the wire contract, [`schemas/harness-config-v1.md`](schemas/harness-config-v1.md) for custom discovery entries, and [`docs/IPC_LIFECYCLE.md`](docs/IPC_LIFECYCLE.md) for supervision/recovery semantics.

## Status

Phase 00–03 implementation and validation. Fake ACP sessions are deterministic end to end; real adapter verification remains conditional on a locally installed adapter. Permission decisions, workspace, capture, and avatar behavior remain intentionally out of scope.
