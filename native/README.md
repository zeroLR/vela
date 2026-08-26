# Native Integrations

Native SDK bridges that should remain isolated from the main Swift application and Rust core.

## Current status

**No native bridge is currently expected.** This directory has no consumer.

It was created for a Live2D Cubism SDK for Native integration behind an Objective-C++
bridge. [`plan/07-avatar-presence.md`](../plan/07-avatar-presence.md) withdrew that
approach: the app is a pure SwiftPM package, and a C++/CMake build with a closed-source
core library inside the signed bundle would invalidate the Phase 00 build assumptions and
add a Phase 09 signing dependency.

Neither Phase 07 route needs this boundary:

- the first avatar adapter is Rive, consumed as a Swift package;
- the deferred Live2D route runs in a `WKWebView`, not a native C++ bridge.

## If a native bridge becomes necessary

The rules that made this boundary worth having still apply:

- vendor types stay inside the bridge; the app consumes Vela-owned abstractions such as
  `AvatarRuntime` and never the SDK's own model, rendering, or parameter types;
- prefer a prebuilt `xcframework` consumed through a SwiftPM `.binaryTarget` over
  introducing an Xcode project;
- verify codesign, hardened runtime, and notarization behavior for any bundled binary
  before depending on it, not after.

Do not add vendor SDK binaries or licensed assets until the relevant integration milestone
and licensing/distribution requirements are explicitly verified.
