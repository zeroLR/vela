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
      "version_arguments": ["--version"],
      "launch_arguments": ["--scenario", "ready"]
    }
  ]
}
```

Fields:

- `id` is a stable lowercase identifier beginning with `a-z`, followed by lowercase letters, digits, or hyphens.
- `display_name` is the user-facing name.
- `command` is an absolute/relative executable path or a command resolved through `PATH` and known macOS binary directories.
- `adapter` defaults to `custom-acp`.
- `version_arguments` defaults to `["--version"]`.
- `launch_arguments` defaults to `[]` and is passed to the ACP process.

Unknown fields, duplicate IDs, built-in ID overrides, and malformed files produce visible failed registry entries. This file is deliberately not a secret store: it has no environment or credential fields. Authentication stays with the provider CLI, and secrets must not be placed in arguments.

See [`../fixtures/harnesses.example.json`](../fixtures/harnesses.example.json) for a copyable template.
