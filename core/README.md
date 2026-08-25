# Vela Core

Rust sidecar runtime boundary.

Expected responsibilities:

- local IPC server
- ACP client/session runtime
- harness discovery
- process supervision
- permission broker domain flow
- workspace engine and filesystem watching
- SQLite-backed operational/event data
- scheduler
- structured diagnostics

The concrete Cargo workspace is intentionally created during `plan/00-foundation.md`. ACP/provider-specific code must remain behind adapter boundaries and must not leak wire types into Vela domain contracts.
