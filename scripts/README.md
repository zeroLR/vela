# Development Scripts

Repository-level automation belongs here.

Expected responsibilities:

- local bootstrap checks
- Swift/Rust build orchestration
- contract-test helpers
- packaging/signing/notarization helpers
- release/update-feed helpers

Scripts should remain thin wrappers around reproducible commands. Core product logic and environment-specific secrets must not live here.
