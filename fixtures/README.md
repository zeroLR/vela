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

The fake ACP harness is a first-class test dependency. It supports discovery failures plus `ready`, `cancel`, `permission`, `prompt-timeout`, `unexpected-exit`, and `malformed-event` session scenarios. Its permission scenario can emit every Phase 04 category through `--permission-kind`; tests cover allow, deny, exact session grants, timeout, cancellation, concurrent sessions, audit history, streaming normalization, PID/process cleanup, terminal invariants, and failure recovery.

`harnesses.example.json` documents the optional user-defined harness registry entry.
