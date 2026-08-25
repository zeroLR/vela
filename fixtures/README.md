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

The fake ACP harness is a first-class test dependency. CI should use it to verify streaming, cancellation, permission, failure recovery, and protocol compatibility before real Claude/Codex smoke tests are considered.
