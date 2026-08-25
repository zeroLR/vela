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

The Cargo workspace now contains the Phase 00–04 execution plane:

- `domain` owns Vela IPC and normalized agent registry types;
- `acp-runtime` owns the official ACP SDK boundary, managed adapter processes, sessions, cancellation, and event normalization;
- `harness-discovery` owns executable/version probes and the cached registry;
- `permission-broker` owns pending requests, exact session grants, safe defaults, and audit history;
- `assistant-ipc` owns the Unix-socket server and Vela protocol handling;
- `vela-core` is the sidecar executable and structured diagnostics entry point.

Build and test it with:

```bash
cargo build --manifest-path core/Cargo.toml --workspace
cargo test --manifest-path core/Cargo.toml --workspace
```

Discovery checks `PATH`, common macOS binary locations, and the optional `VELA_HARNESS_CONFIG` file. Ready adapters can create ACP v1 sessions, stream normalized events, cancel prompts, and pause permission responders until Vela resolves them. Vela does not install adapters or read provider credential stores. ACP/provider-specific wire types remain inside `acp-runtime` and do not leak into Vela domain or IPC contracts.
