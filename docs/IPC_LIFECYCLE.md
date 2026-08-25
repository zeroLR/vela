# Core IPC Lifecycle

Phase 01 uses an explicit-restart policy. The app never hides a Core failure by immediately replacing the process; it presents the degraded state and retains the exit/connection diagnostic. The user can then restart from the diagnostics screen without relaunching Vela.

```mermaid
stateDiagram-v2
    [*] --> Launching
    Launching --> Handshaking: socket appears
    Handshaking --> Ready: compatible core.hello
    Handshaking --> Degraded: timeout or incompatible major
    Ready --> Degraded: Core exits or socket closes
    Degraded --> Launching: explicit restart
    Ready --> Stopped: app terminates
```

Core readiness is the creation of its configured Unix socket. IPC readiness additionally requires a successful `core.hello` response with a compatible major version. A five-second launch timeout terminates the child and surfaces a diagnostic.

Each stream request owns a request ID. Ordered pushed events retain that ID until exactly one `stream.completed` or `stream.cancelled` terminal event. The Swift client rejects later events for a terminal request.

## Orphan prevention

The app terminates Core on quit, but a crashed or `SIGKILL`ed app never runs that
path, and a Core that outlives its supervisor keeps holding the socket. The app
therefore launches Core with `--exit-with-parent`: Core records its parent process
ID at startup and shuts down once that ID changes, or once it is `launchd`, since
both mean the supervising app is gone. Detection takes at most 250ms.

Core does not delete the socket file on that path. The next Core removes a stale
socket when it binds, so an exiting orphan could otherwise delete a successor's
live socket. Without the flag Core keeps running when orphaned, so a manually
launched Core is unaffected.

Development executable discovery order is:

1. `VELA_CORE_PATH`;
2. an executable next to or inside the app bundle;
3. `core/target/debug/vela-core` relative to the current working directory or its parent.
