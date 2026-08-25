# Phase 08 — Scheduled Assistance

## Hypothesis

Scheduled work-state summaries can make Vela proactively useful without requiring continuous background reasoning or intrusive notifications.

## Objectives

- Add a small scheduler in Vela Core.
- Define scheduled jobs independently from macOS notification presentation.
- Generate summaries from current workspace state.
- Route scheduled reasoning through the same agent/runtime boundaries as interactive requests.
- Make scheduled actions observable, cancellable, and safe.

## Initial Use Cases

Examples:

```text
Morning brief
- current focus
- open blockers
- next actions

Afternoon checkpoint
- work that has stalled
- newly created tasks
- unresolved blockers

End-of-day review
- completed work
- remaining work
- suggested carry-over
```

## Scope

- user-defined enable/disable;
- local schedule persistence;
- missed-run behavior documented explicitly;
- scheduled job event history;
- native macOS notification for completed report;
- opening the relevant Vela context from a notification;
- optional ACP reasoning only when the report requires it.

## Non-goals

- Generic cron replacement.
- Sub-hour high-frequency monitoring.
- Arbitrary autonomous shell/tool execution from scheduled jobs.
- Cloud push notifications.
- Team workflows.

## Deliverable

A user can configure at least one recurring work-state report, receive a native notification, open the generated report, and inspect the workspace/event evidence used to produce it.

## Acceptance Criteria

- [ ] Schedules persist across app/core restarts.
- [ ] A scheduled run has a unique job/run ID and structured event trail.
- [ ] Disabled schedules do not execute.
- [ ] Duplicate execution after restart is prevented or explicitly idempotent.
- [ ] Missed-run semantics are defined and tested.
- [ ] Notification content is derived from a durable report artifact/event, not only transient UI state.
- [ ] Scheduled reasoning uses the same permission and ACP safety boundaries as interactive execution.
- [ ] Failure to run an ACP harness does not corrupt scheduler state.
- [ ] User can inspect and delete schedule configuration.
- [ ] Tests cover normal run, restart, missed run, disabled schedule, failure, and duplicate prevention.

## Dogfood Experiment

Use morning and end-of-day reports during normal work.

Track:

- whether reports surface forgotten blockers/tasks;
- notification dismissal rate;
- how often a report causes a useful next action;
- how often reports are redundant with information already visible;
- whether timing feels intrusive.

## Evidence to Capture

- Schedule/run data model.
- Example generated reports.
- Restart/idempotency test output.
- Dogfood examples of useful vs noisy notifications.

## Exit Gate — Persistent Presence

Evaluate:

> Do avatar presence and scheduled assistance increase useful engagement without becoming distracting?

Keep proactive behavior only when it produces measurable value in dogfooding.
