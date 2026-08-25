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
      "capabilities": ["prompt.image", "session.list"],
      "auth_methods": [],
      "diagnostic": null
    }
  ]
}
```

Status is one of `unavailable`, `ready`, `unauthenticated`, `incompatible`, or `failed`. A refresh increments `generation`; clients should ignore an older result arriving after a newer one.

## Stream events

- `stream.chunk` carries `request_id`, one-based `sequence`, and deterministic `text`.
- `stream.completed` and `stream.cancelled` are terminal and carry `request_id` and `emitted`.
- A receiver must ignore events for a request after its first terminal event.

Malformed frames return `malformed_json` with a null request ID. Other protocol errors retain the caller's request ID whenever it was decoded.

See [`../fixtures/ipc/v1-session.ndjson`](../fixtures/ipc/v1-session.ndjson) for a compact request/event trace.
