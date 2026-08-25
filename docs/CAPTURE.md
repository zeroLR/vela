# Capture and Work Utility

Phase 06 adds a fast local capture lane that does not start an ACP session. Swift owns native interaction and audio transcription; Rust owns classification, durable records, routing, corrections, metrics, and workspace events.

## Native interaction

`⌥Space` is registered through Carbon as a system global hotkey. Because launchers or input utilities may consume that combination without reporting a Carbon registration conflict, Vela also registers `⌃⌥V` as a fallback. Either shortcut opens a floating Quick Capture panel across Spaces and explicitly orders it to the foreground. The last successfully opened workspace path is stored in `UserDefaults` and reopened after the next Core handshake, so routine captures do not require navigating the diagnostics window.

The panel supports typed input and a hold-to-talk control. Recording state is explicit. Speech uses `AVAudioEngine` and `SFSpeechRecognizer`, and requests microphone and Speech authorization explicitly so a denial becomes a visible failure instead of a silent recording.

Usage descriptions live in `VelaApp.app/Contents/Info.plist`. Start the app with `scripts/run-app.sh`, which builds, ad-hoc signs, and verifies that local bundle before launching it. macOS TCC aborts a process outright when it cannot read a usage description, so pressing hold-to-talk first checks that the running process is inside an `.app` bundle. A bare `swift run` executable still answers the usage-description keys from its embedded `__info_plist` section, which is why the bundle layout — not key presence — is the deciding check; on that launch path push-to-talk reports the required launch command instead of reaching TCC.

The panel also answers the current-state questions inline — working on, blocked,
next — from `workspace.current_state`, so `⌥Space` covers both capture and "where am
I" without opening the diagnostics window. Answers refresh after every capture and
carry the age of `STATUS.md`, because stale state is misleading state. See
[`WORKSPACE.md`](WORKSPACE.md) for the derivation rules.

The transcript remains editable before submission. Audio is written to a temporary CAF file while recording. On a recognition failure or abandoned speech capture, the UI displays/retains that path so a partial transcript or recording is not silently lost. After a successful capture is acknowledged and dismissed, the temporary audio is removed.

## Canonical capture record

Every completed or abandoned attempt is stored as human-readable JSON at:

```text
captures/<capture-id>.json
```

The record retains:

- source (`text` or `speech`) and status (`completed` or `abandoned`);
- original raw text and whitespace-normalized text;
- deterministic suggested intent and current user-selected intent;
- title and routed workspace path;
- start/completion timestamps and correction count.

The original suggestion is not overwritten when the user corrects a route. Records survive Core/app restarts and do not depend on chat history or SQLite.

## Fast classification and routing

The initial intent vocabulary is `note`, `idea`, `todo`, `work_update`, and `unknown`. Core applies a deliberately small prefix-based classifier for obvious English and Traditional Chinese capture phrases. The user can choose an explicit route before submission or correct it afterward.

Routing is file-oriented:

| Intent | Route |
|---|---|
| `note`, `idea` | `notes/<capture-id>.md` |
| `todo` | `tasks/<capture-id>.md` |
| `work_update` | A capture-marked entry in `STATUS.md` |
| `unknown` | A capture-marked entry in `INBOX.md` |

Corrections remove only the artifact/marker owned by that capture and create the new route. Raw input and the canonical capture record remain intact. Work-update markers are returned by the existing status-first context API, so current focus/blocker/next-action queries can see them without loading a capture archive.

## Events and local metrics

Explicit capture operations carry the IPC request ID as correlation and `user` provenance. The event stream records `capture.created`, `capture.routed`, `capture.corrected`, `capture.abandoned`, `task.created`, and `state.updated` in addition to underlying file changes.

Metrics are derived from canonical capture records:

- total/completed/abandoned captures;
- captures since a caller-supplied timestamp (Swift supplies local start-of-day);
- corrected captures and correction rate in basis points;
- median completion time.

No telemetry leaves the machine.

## Validation boundary

Automated coverage proves raw/structured preservation, reopen, deterministic routing, route correction and cleanup, abandonment, metrics, event correlation, and Swift→Core operation against a real process and Unix socket. The native executable starts and supervises Core. `scripts/run-app.sh` refuses to launch a bundle whose Info.plist is missing either privacy key.

Gate B still requires user-driven macOS validation: invoke `⌥Space` and fallback `⌃⌥V` while another app has focus, approve/deny microphone and Speech permissions, exercise success/failure/cancel flows, then dogfood across multiple normal work sessions. Screen/Accessibility automation is intentionally not used to bypass those permissions.
