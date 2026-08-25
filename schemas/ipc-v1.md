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
| `stream.start` | `count`, `interval_ms` | Stream acceptance; subsequent events use this request ID |
| `stream.cancel` | `target_request_id` | Whether cancellation was requested |

## Stream events

- `stream.chunk` carries `request_id`, one-based `sequence`, and deterministic `text`.
- `stream.completed` and `stream.cancelled` are terminal and carry `request_id` and `emitted`.
- A receiver must ignore events for a request after its first terminal event.

Malformed frames return `malformed_json` with a null request ID. Other protocol errors retain the caller's request ID whenever it was decoded.

See [`../fixtures/ipc/v1-session.ndjson`](../fixtures/ipc/v1-session.ndjson) for a compact request/event trace.
