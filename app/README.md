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

The Xcode project and concrete source tree are intentionally deferred to `plan/00-foundation.md` so the first implementation can validate the minimum useful structure instead of committing speculative boilerplate.
