# Vela Harness Configuration v1

Vela always registers built-in definitions for `codex-acp` and `claude-agent-acp`. An optional JSON file can add ACP-compatible harnesses without adding provider logic to the Swift app.

Set its absolute path before starting Vela Core:

```bash
export VELA_HARNESS_CONFIG=/absolute/path/to/harnesses.json
```

```json
{
  "harnesses": [
    {
      "id": "fake-local",
      "display_name": "Fake Local Agent",
      "command": "/absolute/path/to/fake-acp-harness",
      "adapter": "test-acp",
      "enforced_session_mode": "safe",
      "version_arguments": ["--version"],
      "launch_arguments": ["--scenario", "ready"],
      "launch_environment": {"EXAMPLE_NON_SECRET_FLAG": "enabled"}
    }
  ]
}
```

Fields:

- `id` is a stable lowercase identifier beginning with `a-z`, followed by lowercase letters, digits, or hyphens.
- `display_name` is the user-facing name.
- `command` is an absolute/relative executable path or a command resolved through `PATH` and known macOS binary directories.
- `adapter` defaults to `custom-acp`.
- `enforced_session_mode` is required and must be a non-empty ACP mode ID advertised by `session/new`. Vela sets it before declaring a session ready; failure rejects the session.
- `version_arguments` defaults to `["--version"]`.
- `launch_arguments` defaults to `[]` and is passed to the ACP process.
- `launch_environment` defaults to `{}` and overrides environment variables only for the adapter child process.

Unknown fields, duplicate IDs, built-in ID overrides, missing enforcement modes, and malformed files produce visible failed registry entries. This file is deliberately not a secret store. Authentication stays with the provider CLI, and secrets must not be placed in arguments or `launch_environment`.

See [`../fixtures/harnesses.example.json`](../fixtures/harnesses.example.json) for a copyable template.
