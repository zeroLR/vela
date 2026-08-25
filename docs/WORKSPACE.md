# Local-First Workspace

Phase 05 makes a user-selected directory Vela's durable work-state boundary. The filesystem is canonical. SQLite is a disposable index and event log that can be rebuilt from the canonical files and reference manifest.

## Canonical layout

Opening a directory creates only missing artifacts:

```text
workspace/
├── STATUS.md
├── INBOX.md
├── projects/
├── tasks/
├── notes/
├── context/
│   └── REFERENCES.json
├── decisions/
├── evidence/
└── .vela/
    ├── workspace.json
    └── index.sqlite3
```

`STATUS.md`, `INBOX.md`, the content directories, `context/REFERENCES.json`, and `.vela/workspace.json` are canonical. `.vela/index.sqlite3` is derived. Existing files are never overwritten when a workspace is reopened.

`STATUS.md` is the compact first context layer: active focus, blockers, and next actions. `INBOX.md` holds unprocessed inputs. The other directories deliberately remain Markdown/file-oriented until real dogfooding justifies a stricter schema.

## References

`context/REFERENCES.json` stores stable IDs and canonical paths for explicitly added external directories. Referenced content stays in place; Vela neither copies it into the workspace nor deletes it when the reference is removed.

Scanning ignores symlinks and common derived trees (`.git`, `.build`, `target`, and `node_modules`). Context reads canonicalize both the configured root and requested file, so `..` and symlink traversal cannot escape the selected workspace or reference.

## Derived SQLite state

The database contains three tables:

- `files`: `(source, path)` plus size and nanosecond modification time.
- `reference_index`: a queryable copy of the canonical reference manifest.
- `workspace_events`: ordered timestamp, kind, optional path, provenance, and correlation ID.

The engine opens short-lived SQLite connections and uses a busy timeout. Deleting `.vela/index.sqlite3` is supported: reopening the workspace recreates the schema, scans canonical workspace/reference files, restores the derived reference index, and records `workspace.index_rebuilt`.

Workspace event kinds include `workspace.created`, `workspace.index_rebuilt`, `workspace.file_changed`, `reference.added`, and `reference.removed`. Phase 06 adds `capture.created`, `capture.routed`, `capture.corrected`, `capture.abandoned`, `task.created`, and `state.updated` through their owning operations.

Provenance is one of `user`, `agent`, `tool`, `scheduler`, `external_filesystem`, or `system`. IPC writes accept the first four; watcher observations and index lifecycle events are assigned by Core. The originating IPC request ID is retained as the correlation ID for explicit writes and reference changes.

## Reconcile watcher

Opening a workspace starts one polling reconcile loop with a 500 ms interval. Opening another workspace aborts the previous loop. Each scan compares stable file metadata with the derived index and emits events only for additions, changes, or removals. A Vela write reconciles immediately, so the following watcher pass sees an identical index and does not create an event loop.

Polling is intentional for the first vertical slice: it gives deterministic recovery from missed notifications and external editor/CLI changes without making an OS-specific watcher abstraction part of the domain contract.

## Progressive context

Context is explicit and bounded to 32 KiB per requested file:

1. `status` loads `STATUS.md` and `INBOX.md`.
2. `workspace_path` loads one named file inside the workspace.
3. `reference_path` loads one named file inside one explicit reference.

Core returns a `truncated` flag instead of silently expanding a large file. It never recursively injects a repository into an agent prompt.

## Recovery procedure

1. Stop Core or close the active workspace.
2. Delete only `workspace/.vela/index.sqlite3` and its SQLite sidecars if present.
3. Reopen the same workspace through `workspace.open`, or call `workspace.rebuild` while it is active when the database is readable.
4. Confirm `indexed_file_count`, references, and a `workspace.index_rebuilt` event.

Do not delete `context/REFERENCES.json` or `.vela/workspace.json`; they are canonical metadata.

## Verification coverage

Rust tests prove create/reopen durability, index deletion/rebuild, external mutation detection, watcher recovery, loop-free self writes, bounded traversal-safe context, and non-destructive reference removal. IPC tests exercise those operations over a real Unix socket. The Swift/Rust integration test drives the same slice from `IPCClient` against a real `vela-core` process.
