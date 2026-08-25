# Vela Local IPC v1

Vela App and Vela Core exchange UTF-8 JSON objects separated by a newline over a Unix domain socket. Every message includes:

```json
{ "version": { "major": 1, "minor": 0 } }
```

Peers accept different minor versions and reject different major versions. Requests carry a unique `id`, a `method`, and an object-valued `params`. Responses repeat the request `id` and contain exactly one of `result` or `error`. Server events contain `event` and `data`.

## Methods

| Method | Parameters | Result |
|---|---|---|
| `core.hello` | `{}` | Core name/version and negotiated protocol version |
| `core.health` | `{}` | `{ "status": "healthy" }` |
| `agents.list` | `{}` | Last cached `AgentRegistrySnapshot`; does not launch probes |
| `agents.refresh` | `{}` | Runs discovery and returns the new `AgentRegistrySnapshot` |
| `workspace.open` | `root` | Creates/reopens a workspace and returns `WorkspaceSnapshot` |
| `workspace.status` | `{}` | Returns the active `WorkspaceSnapshot` without reconciling |
| `workspace.refresh` | `{}` | Reconciles external changes and returns `WorkspaceSnapshot` |
| `workspace.write` | `path`, `content`, optional `provenance` | Atomically writes a canonical workspace file and returns `WorkspaceSnapshot` |
| `workspace.reference.add` | `path` | Adds an external directory reference and returns `WorkspaceSnapshot` |
| `workspace.reference.remove` | `reference_id` | Removes a reference without deleting external content |
| `workspace.events` | optional `limit` | Returns up to 500 ordered workspace events |
| `workspace.rebuild` | `{}` | Rebuilds the derived SQLite index and returns `WorkspaceSnapshot` |
| `workspace.context` | `scope`, optional `path`/`reference_id` | Returns an explicit bounded `WorkspaceContextSlice` |
| `session.create` | `agent_id`, optional `cwd` | Creates an ACP process/session and returns a `SessionDescriptor` |
| `session.get` | `session_id` | Returns the current `SessionDescriptor` |
| `session.prompt` | `session_id`, `text` | Accepts a run and returns `run_id` plus the ACP request correlation ID |
| `session.cancel` | `session_id`, `run_id` | Requests ACP `session/cancel` for the active run |
| `permissions.pending` | optional `session_id` | Returns the current Vela-owned permission request queue |
| `permissions.history` | optional `session_id` | Returns structured permission audit records |
| `permission.resolve` | `permission_id`, `session_id`, `run_id`, `decision` | Resolves one pending request; decision is `allow_once`, `allow_session`, or `deny` |
| `stream.start` | `count`, `interval_ms` | Stream acceptance; subsequent events use this request ID |
| `stream.cancel` | `target_request_id` | Whether cancellation was requested |

`AgentRegistrySnapshot` is a Vela-owned contract, not an ACP wire type:

```json
{
  "generation": 1,
  "refreshed_at_ms": 1787661000000,
  "agents": [
    {
      "id": "codex",
      "display_name": "Codex",
      "adapter": "codex-acp",
      "source": "built_in",
      "status": "ready",
      "executable_path": "/opt/homebrew/bin/codex-acp",
      "version": "codex-acp 0.8.1",
      "protocol_version": "1",
      "enforced_session_mode": "read-only",
      "capabilities": ["prompt.image", "session.list"],
      "auth_methods": [],
      "diagnostic": null
    }
  ]
}
```

Status is one of `unavailable`, `ready`, `unauthenticated`, `incompatible`, or `failed`. `enforced_session_mode` is the exact ACP mode Vela must successfully set after `session/new` and before returning a ready `SessionDescriptor`. A refresh increments `generation`; clients should ignore an older result arriving after a newer one.

## Workspace contract

`WorkspaceSnapshot` contains the canonical root, creation timestamp, current `STATUS.md` and `INBOX.md` text, explicit references, derived indexed-file count, and latest event ID. Filesystem content and `context/REFERENCES.json` are canonical; SQLite details do not cross IPC.

`workspace.write` provenance may be `user`, `agent`, `tool`, or `scheduler` and defaults to `user`. Core assigns `external_filesystem` and `system`. Workspace operation failures use `workspace_error` and retain the request ID.

`workspace.context` scopes are:

- `status`: no additional parameters; returns status and inbox.
- `workspace_path`: requires one relative `path`.
- `reference_path`: requires `reference_id` and a relative `path`.

Each returned file is capped at 32 KiB and carries `truncated`. Paths cannot traverse outside their selected root. See [`../docs/WORKSPACE.md`](../docs/WORKSPACE.md) for ownership, indexing, watcher, and recovery semantics.

## Agent events

After `session.prompt` succeeds, Core pushes `agent.event` messages. The `data` object always contains `session_id`, `run_id`, originating IPC `request_id`, one-based `sequence`, and `timestamp_ms`. Its Vela-owned `kind` is one of:

- `text_delta`
- `plan_updated`
- `tool_started`
- `tool_finished`
- `permission_requested`
- `permission_resolved`
- `usage_updated`
- `completed`
- `cancelled`
- `failed`

```json
{
  "version": { "major": 1, "minor": 0 },
  "event": "agent.event",
  "data": {
    "session_id": "session-123-1",
    "run_id": "run-123-2",
    "request_id": "prompt-request-1",
    "sequence": 1,
    "timestamp_ms": 1787663385500,
    "kind": "text_delta",
    "text": "Hello"
  }
}
```

`completed`, `cancelled`, and `failed` are terminal. Core emits exactly one terminal event per accepted run; clients must ignore any later event carrying that run ID.

## Permission contract

`permission_requested` contains a nested `request` with a Vela permission ID, agent/session/run/request provenance, tool-call ID, normalized category, title, optional target, ACP option metadata, and creation/expiry timestamps. Categories are `filesystem.read`, `filesystem.write`, `shell.execute`, `network.open_url`, `mcp.invoke`, and `other`.

`permission_resolved` contains a nested audit `record`. Its status is `allowed`, `denied`, `timed_out`, or `cancelled`; source is `user`, `session_grant`, `timeout`, or `cancellation`. Stale, duplicate, or provenance-mismatched `permission.resolve` requests fail with `permission_resolution_failed`.

`Allow for session` is deliberately narrower than ACP `allow_always`: Vela records an exact `(session, category, title, target)` grant and selects an ACP `allow_once` option for each matching request. Grants are deleted with the Vela session and never leak to another session. A missing allow-once option disables both Vela allow decisions. Deny selects ACP `reject_once` when offered; timeout, cancellation, or absence of a safely scoped option returns ACP `cancelled`.

## Stream events

- `stream.chunk` carries `request_id`, one-based `sequence`, and deterministic `text`.
- `stream.completed` and `stream.cancelled` are terminal and carry `request_id` and `emitted`.
- A receiver must ignore events for a request after its first terminal event.

Malformed frames return `malformed_json` with a null request ID. Other protocol errors retain the caller's request ID whenever it was decoded.

See [`../fixtures/ipc/v1-session.ndjson`](../fixtures/ipc/v1-session.ndjson) for a compact request/event trace.
