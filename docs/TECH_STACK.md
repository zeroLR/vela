# Vela Tech Stack

## Recommended Stack

| Layer | Technology | Responsibility |
|---|---|---|
| macOS UI | Swift, SwiftUI | Main application UI and state presentation |
| macOS integration | AppKit | Windowing, menu bar, floating panels, desktop-specific behavior |
| Native concurrency | Swift async/await, Observation | UI-side asynchronous state and bindings |
| Core runtime | Rust, Tokio | Agent orchestration, process lifecycle, workspace, scheduling, events |
| ACP | Official ACP Rust SDK | Agent client/session/runtime integration |
| Local IPC | Unix Domain Socket + JSON | Swift ↔ Rust communication |
| Workspace | Filesystem | Canonical local-first work state |
| Index/event DB | SQLite, rusqlite | Events, indexes, session metadata, schedules, caches |
| Avatar | `AvatarRuntime` abstraction | Stable semantic avatar interface |
| Initial avatar adapter | Rive (`rive-app/rive-ios`, MIT) | Rendering, state-machine reactions, lip-sync input |
| Deferred avatar adapter | Live2D Cubism SDK for Web in `WKWebView` | Intended second adapter; gated on licensing and resource cost |
| Audio | AVFoundation | Microphone capture and audio playback |
| Speech | Adapter-based | Apple/local/cloud STT and TTS implementations |
| Secrets | macOS Keychain | Vela-owned secrets only |
| Notifications | UserNotifications | Scheduled and proactive reports |
| Logging | `tracing`, `tracing-subscriber` | Structured core diagnostics |
| Distribution | Developer ID, Hardened Runtime, Notarization | Direct macOS distribution |
| Updates | Sparkle 2 | Application updates outside the Mac App Store |
| CI | GitHub Actions | Swift/Rust tests, contract tests, signed builds |

## Why Swift + Rust

### Swift side

Own anything that is primarily an operating-system or presentation concern:

- SwiftUI/AppKit UI;
- global hotkeys;
- microphone and playback;
- native notifications;
- Keychain;
- avatar presentation;
- macOS permission UX.

### Rust side

Own long-lived, concurrent, testable runtime concerns:

- ACP protocol/session lifecycle;
- harness discovery;
- process supervision;
- cancellation and streaming;
- permission requests;
- workspace watching/indexing;
- event history;
- scheduling;
- future headless/CLI support.

## IPC Strategy

Start with a deliberately simple local protocol:

```text
vela.app
   |
   | Unix Domain Socket
   | JSON request/response + pushed events
   v
vela-core
```

Do not introduce gRPC, protobuf, or FFI until profiling or product requirements justify them.

IPC schemas must be Vela-owned and versioned. ACP wire models stay behind the Rust adapter.

## Repository Layout

Target layout:

```text
vela/
├── app/
│   ├── README.md
│   ├── Sources/
│   └── Tests/
├── core/
│   ├── README.md
│   ├── Cargo.toml
│   └── crates/
├── native/
│   └── README.md
├── schemas/
│   └── README.md
├── fixtures/
│   └── README.md
├── docs/
├── plan/
└── scripts/
```

The initial repository only creates lightweight placeholders for these boundaries. Tool-generated Xcode and Cargo structures should be added in the corresponding validation milestone rather than prematurely committing speculative boilerplate.

## Rust Workspace Direction

Expected crate boundaries:

```text
core/crates/
├── assistant-core/
├── assistant-ipc/
├── domain/
├── agent-runtime/
├── acp-runtime/
├── harness-discovery/
├── process-runtime/
├── workspace/
├── event-store/
└── scheduler/
```

Do not create these crates until the milestone that proves their responsibility. Early milestones may combine responsibilities temporarily and split them only once interfaces are exercised.

## Storage Rules

### Filesystem is canonical

Human-readable work artifacts remain files so they can be:

- read by users;
- inspected by Claude/Codex;
- versioned with Git when desired;
- edited by external tools;
- exported without a database migration.

### SQLite is derived/operational state

Use SQLite for data with database-shaped access patterns:

- event log;
- session records;
- indexes;
- schedules;
- caches;
- referenced-folder metadata.

Avoid adding a vector database in the MVP. Progressive disclosure and explicit workspace structure should be validated first.

## ACP Rules

- ACP is the primary agent execution interface.
- Detect Claude/Codex-compatible local tooling rather than embedding provider-specific APIs first.
- Do not read private CLI credential stores.
- Normalize ACP events into stable Vela domain events.
- Keep protocol-version changes inside adapters.
- Use a fake ACP harness for deterministic tests.

## Security Rules

- Agent execution must not bypass the permission broker.
- Secrets owned by Vela belong in Keychain, not plaintext config.
- External CLI authentication remains owned by that CLI when possible.
- Workspace mutation events should retain provenance.
- Referenced folders are explicit capabilities, not ambient access assumptions.

## Deferred Technology

Do not add these during the initial validation path unless a milestone proves they are necessary:

- Electron or Tauri as the main UI runtime;
- direct OpenAI/Anthropic SDK integration;
- LangChain/LangGraph orchestration;
- PostgreSQL;
- vector database;
- gRPC;
- custom MCP execution runtime;
- multi-agent graph orchestration;
- full-duplex always-listening voice.

The deferred Live2D avatar adapter would run inside a `WKWebView`. That is not an exception to the first item: the WebView would render the avatar only and expose a single avatar command channel. Hosting Vela UI in a web runtime remains deferred and requires its own justification.
