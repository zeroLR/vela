# Native Integrations

Native SDK bridges that should remain isolated from the main Swift application and Rust core.

Initial expected integration:

- Live2D Cubism SDK for Native
- Objective-C++/C++ bridge where required

The app should consume a Vela-owned `AvatarRuntime` abstraction. Cubism-specific model/rendering types must stay inside this boundary.

Do not add vendor SDK binaries or licensed assets until the relevant integration milestone and licensing/distribution requirements are explicitly verified.
