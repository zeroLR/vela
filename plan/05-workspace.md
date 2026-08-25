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

- [ ] Workspace can be created and reopened without data loss.
- [ ] Core treats filesystem artifacts as canonical work content.
- [ ] SQLite can be deleted/rebuilt without losing canonical workspace data.
- [ ] A referenced folder can be added and removed without copying or deleting its contents.
- [ ] External edits are detected and indexed.
- [ ] Self-generated edits do not produce unbounded watcher loops.
- [ ] Events contain timestamp, type, correlation/provenance where applicable.
- [ ] `STATUS.md` or equivalent compact state can answer active focus, blockers, and next actions.
- [ ] Agent context requests can load status first and expand into deeper project/context/evidence layers on demand.
- [ ] Tests cover reopen, index rebuild, external mutation, reference removal, and watcher recovery.

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

## Exit Decision

Proceed when the workspace can be trusted as durable state independently of chat sessions and derived indexes.
