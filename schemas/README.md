# Shared Schemas

Versioned contracts shared across runtime boundaries.

Initial responsibility:

- Vela local IPC request/response/event schemas
- schema versioning rules
- example payloads used by Swift/Rust contract tests

Rules:

- Schemas belong to Vela, not ACP.
- ACP wire types must be normalized inside the Rust adapter before crossing IPC.
- Breaking schema changes require an explicit major-version strategy.
- Example fixtures should accompany schema changes.

The concrete contracts are [`ipc-v1.md`](ipc-v1.md) and [`harness-config-v1.md`](harness-config-v1.md). IPC deliberately uses a small NDJSON envelope until later product requirements justify a heavier transport.
