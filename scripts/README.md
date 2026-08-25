# Development Scripts

Repository-level automation belongs here.

Expected responsibilities:

- local bootstrap checks
- Swift/Rust build orchestration
- contract-test helpers
- packaging/signing/notarization helpers
- release/update-feed helpers

Scripts should remain thin wrappers around reproducible commands. Core product logic and environment-specific secrets must not live here.

Run all Phase 00/01 checks from the repository root:

```bash
scripts/check.sh
```

Launch the desktop app from the repository root:

```bash
scripts/run-app.sh              # build the .app bundle and launch it detached
scripts/run-app.sh --attached   # keep app and Core logs in this terminal
scripts/run-app.sh --bundle-only
```

`run-app.sh` exists because macOS TCC only reads microphone and Speech usage
descriptions from a real bundle's `Info.plist`; push-to-talk stays disabled under
a bare `swift run`.
