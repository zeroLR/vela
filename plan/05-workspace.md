# Phase 05 — Local-First Workspace

## Hypothesis

A human-readable filesystem workspace plus derived SQLite indexes/events can maintain useful work state without requiring conversation history or a vector database as the source of truth.

## Objectives

- Create a Vela-owned workspace root.
- Support explicitly referenced external folders.
- Define current work state, inbox, tasks, notes, projects, context, decisions, and evidence boundaries.
- Track filesystem changes and provenance.
- Add SQLite for operational metadata and event history.
- Expose progressive context reads to the agent runtime.

## Initial Workspace Shape

```text
workspace/
├── STATUS.md
├── INBOX.md
├── projects/
├── tasks/
├── notes/
├── context/
├── decisions/
└── evidence/
```

Exact schemas should remain minimal until dogfooding produces real examples.

## Scope

### Workspace ownership

- Vela owns its workspace root.
- Referenced folders remain in place.
- External folder references are explicit and removable.
- Files created by users or external agents should be detected by the watcher.

### Event history

At minimum capture:

```text
workspace.created
workspace.file_changed
reference.added
reference.removed
capture.created
task.created
task.updated
state.updated
agent.mutation_observed
```

### Provenance

Changes derived by Vela should preserve whether they originated from user input, agent inference, tool output, scheduler, or external filesystem change.

## Non-goals

- Cloud sync.
- Semantic/vector indexing.
- Automatic ingestion of entire repositories into prompts.
- A complex task-management schema.
- Multi-user collaboration.

## Deliverable

Vela can create/open its workspace, add a referenced project folder, observe changes from both Vela and external tools, display a compact current-work-state view, and provide bounded context slices to an agent session.

## Acceptance Criteria

- [x] Workspace can be created and reopened without data loss.
- [x] Core treats filesystem artifacts as canonical work content.
- [x] SQLite can be deleted/rebuilt without losing canonical workspace data.
- [x] A referenced folder can be added and removed without copying or deleting its contents.
- [x] External edits are detected and indexed.
- [x] Self-generated edits do not produce unbounded watcher loops.
- [x] Events contain timestamp, type, correlation/provenance where applicable.
- [x] `STATUS.md` or equivalent compact state can answer active focus, blockers, and next actions.
- [x] Agent context requests can load status first and expand into deeper project/context/evidence layers on demand.
- [x] Tests cover reopen, index rebuild, external mutation, reference removal, and watcher recovery.

## Validation Procedure

1. Create a new workspace and seed a small project.
2. Add a real local repository as a reference.
3. Edit files from Finder/editor/CLI while Vela is running.
4. Verify watcher/index/event behavior.
5. Remove SQLite and rebuild derived data.
6. Ask an agent for current status using only the compact state first.
7. Request deeper evidence and verify context expansion is explicit.

## Evidence to Capture

- Real dogfood workspace examples.
- Event examples with provenance.
- Index rebuild procedure and duration.
- Context-selection trace showing progressive disclosure.

## Validation Record — 2026-08-25

- `workspace-engine` tests create and reopen canonical state, delete/rebuild SQLite, detect an external edit through the polling watcher, remove a reference without deleting its file, and reject traversal/symlink context reads.
- The workspace IPC test uses a real Unix socket to open, write status, add/read/remove a reference, and verify request correlation in the event stream.
- The Swift/Rust integration test starts a real `vela-core`, drives the workspace through `IPCClient`, verifies status/reference/context/event state, and confirms referenced content survives removal.
- The compact SwiftUI workspace panel exposes open/create, reconcile, rebuild, status editing, references, context, and event loading without adding a recursive file browser.
- Implementation and recovery rules are recorded in [`../docs/WORKSPACE.md`](../docs/WORKSPACE.md).

## Exit Decision

**Pass on 2026-08-25.** The filesystem workspace and reference manifest reopen independently of chat sessions, and the SQLite index can be deleted and rebuilt without loss of canonical state. Proceed to Phase 06 — Capture.
