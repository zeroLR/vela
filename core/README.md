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

The Cargo workspace now contains the Phase 00/01 skeleton:

- `domain` owns the IPC protocol version primitive;
- `assistant-ipc` owns the Unix-socket server and Vela protocol handling;
- `vela-core` is the sidecar executable and structured diagnostics entry point.

Build and test it with:

```bash
cargo build --manifest-path core/Cargo.toml --workspace
cargo test --manifest-path core/Cargo.toml --workspace
```

ACP/provider-specific code must remain behind future adapter boundaries and must not leak wire types into Vela domain contracts.
