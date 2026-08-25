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

The Phase 00–06 Swift package contains a native SwiftUI diagnostics app, an IPC client, a `vela-core` process supervisor, and a floating text/speech capture panel. `⌥Space` opens Quick Capture, with `⌃⌥V` as a conflict-resistant fallback; the last workspace reopens after the Core handshake. Typed or push-to-talk input can be reviewed, routed, and corrected without provider-specific Swift logic or an ACP session.

From the repository root:

```bash
cargo build --manifest-path core/Cargo.toml --workspace
swift test --package-path app
scripts/run-app.sh
```

The debug screen can inspect capture metrics/routes, run and cancel a deterministic 20-event stream, terminate Core, and explicitly restart/reconnect without relaunching the app. The supervisor also checks the development build location when `VELA_CORE_PATH` is not set. Microphone and Speech permissions are requested only when the push-to-talk control is used. Run the app through `scripts/run-app.sh` for voice capture (`--attached` keeps app and Core logs in the terminal); a bare `swift run` process deliberately disables it before macOS TCC can abort the process.
