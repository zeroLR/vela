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

The first concrete schema is introduced in `plan/01-core-ipc.md`.
