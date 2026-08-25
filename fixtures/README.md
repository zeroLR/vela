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

The fake ACP harness is a first-class test dependency. It supports discovery failures plus Phase 03 `ready`, `cancel`, `permission`, `prompt-timeout`, `unexpected-exit`, and `malformed-event` session scenarios. Tests cover streaming normalization, cancellation, PID/process cleanup, terminal invariants, and failure recovery; real Claude and Codex smoke tests then validate adapter compatibility without replacing deterministic coverage.

`harnesses.example.json` documents the optional user-defined harness registry entry.
