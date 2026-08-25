# macOS App

Native Vela application boundary.

Expected responsibilities:

- SwiftUI application UI
- AppKit-specific window/menu-bar behavior
- global hotkeys
- AVFoundation audio capture/playback
- native notifications
- Keychain access for Vela-owned secrets
- avatar presentation
- local IPC client for Vela Core

Do not place ACP/provider protocol logic here. The app consumes Vela-owned IPC/domain events only.

The Phase 00–04 Swift package contains a native SwiftUI diagnostics app, an IPC client, and a `vela-core` process supervisor. The diagnostics screen discovers agents, creates a selected ACP session, submits text, renders Vela-owned streaming events, cancels the active run, and queues native permission decisions without provider-specific Swift logic.

From the repository root:

```bash
cargo build --manifest-path core/Cargo.toml --workspace
swift test --package-path app
VELA_CORE_PATH="$PWD/core/target/debug/vela-core" swift run --package-path app VelaApp
```

The debug screen can run and cancel a deterministic 20-event stream, terminate Core, and explicitly restart/reconnect without relaunching the app. The supervisor also checks the development build location when `VELA_CORE_PATH` is not set.
