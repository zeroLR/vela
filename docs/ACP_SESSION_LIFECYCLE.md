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

On the Phase 03 validation machine, the Codex and Claude provider CLIs are installed, but `codex-acp` and `claude-agent-acp` are not. Vela therefore reports both definitions as unavailable and does not auto-install them. A harmless real prompt smoke test remains conditional on a user-installed adapter.
