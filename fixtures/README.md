# Test Fixtures

Deterministic fixtures used to validate Vela runtime behavior without depending on live providers or user environments.

Expected fixtures:

- fake ACP harness
- scripted ACP event streams
- permission-request scenarios
- malformed/timeout/failure scenarios
- sample workspaces
- workspace mutation cases
- schema examples

The fake ACP harness is a first-class test dependency. Phase 02 builds it as `fake-acp-harness`; it supports `ready`, `unauthenticated`, `incompatible`, `invalid`, and `timeout` initialization scenarios plus PID capture for cleanup assertions. Later phases can extend it for streaming, cancellation, and permission flows before real Claude/Codex smoke tests are considered.

`harnesses.example.json` documents the optional user-defined harness registry entry.
