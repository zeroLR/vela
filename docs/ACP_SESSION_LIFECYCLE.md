# ACP Session Lifecycle

Phase 03 keeps provider and ACP wire semantics inside Rust:

```text
ready AgentDescriptor
        |
        v
session.create -> spawn adapter -> ACP initialize -> session/new
        |                                      |
        |                                      v
        |                              SessionDescriptor ready
        v
session.prompt -> ACP session/prompt -> agent.event* -> one terminal event
        |                                      |
        +-> session.cancel -> ACP session/cancel+
```

The normalized lifecycle is `starting → ready → running → completed | cancelled | failed`. A completed, cancelled, or failed session object may start another prompt while its ACP process remains healthy. If the adapter process exits or the protocol becomes malformed, that session fails and a new session can be created without restarting Vela Core.

Every event includes Vela session/run IDs, the originating IPC request ID, the ACP request ID in prompt acceptance diagnostics, adapter PID, sequence, and timestamp. ACP session updates are converted to Vela-owned text, plan, tool, permission, usage, and terminal payloads before IPC.

## Fake scenarios

`fake-acp-harness --scenario <name>` supports:

| Scenario | Behavior |
|---|---|
| `ready` | Text, plan, tool, usage, text, then `end_turn` |
| `cancel` | Emits progress and waits for `session/cancel` |
| `permission` | Requests permission; Vela surfaces it and responds `cancelled` |
| `prompt-timeout` | Ignores the prompt and cancellation |
| `unexpected-exit` | Exits with status 17 during a prompt |
| `malformed-event` | Writes invalid JSON and exits |
| `timeout`, `invalid`, `unauthenticated`, `incompatible` | Phase 02 initialization failures |

The deterministic suite proves ordering, one-terminal-event behavior, cleanup, cancellation, and creating a fresh session after failure.

## Real adapter note

Real adapter smoke tests passed on 2026-08-25:

| Adapter | Provider CLI | Discovery | Normalized trace | Result |
|---|---|---|---|---|
| `codex-acp` 1.6.2 | `codex-cli` 0.149.1 | `ready` | text deltas, usage, completed | `VELA_SMOKE_OK`, `EndTurn` |
| `claude-agent-acp` 0.70.0 | Claude Code 2.1.245 | `ready` | usage, text deltas, completed | `VELA_SMOKE_OK`, `EndTurn` |

Each adapter was launched through Vela discovery and `session.create`, accepted a prompt through `session.prompt`, returned an ACP request ID, and emitted exactly one normalized terminal event. The harmless prompt explicitly prohibited tools and file changes; neither run requested permission, invoked a tool, or modified the workspace. Test processes and the temporary IPC socket were removed after validation.
