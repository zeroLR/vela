use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use acp_runtime::SessionManager;
use domain::{PermissionDecision, ProtocolVersion};
use harness_discovery::{DiscoveryOptions, DiscoveryService};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixListener, UnixStream},
    sync::{watch, Mutex},
    task::JoinSet,
};

type SharedWriter = Arc<Mutex<OwnedWriteHalf>>;
type ActiveStreams = Arc<Mutex<HashMap<String, watch::Sender<bool>>>>;

#[derive(Debug, Deserialize)]
struct Request {
    version: ProtocolVersion,
    id: String,
    method: String,
    #[serde(default)]
    params: Value,
}

pub async fn serve(socket_path: impl AsRef<Path>) -> io::Result<()> {
    serve_with_services(
        socket_path,
        Arc::new(DiscoveryService::new(DiscoveryOptions::from_environment())),
        Arc::new(SessionManager::default()),
    )
    .await
}

pub async fn serve_with_discovery(
    socket_path: impl AsRef<Path>,
    discovery: Arc<DiscoveryService>,
) -> io::Result<()> {
    serve_with_services(socket_path, discovery, Arc::new(SessionManager::default())).await
}

pub async fn serve_with_services(
    socket_path: impl AsRef<Path>,
    discovery: Arc<DiscoveryService>,
    sessions: Arc<SessionManager>,
) -> io::Result<()> {
    let socket_path = socket_path.as_ref();
    if socket_path.exists() {
        std::fs::remove_file(socket_path)?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(socket_path)?;
    tracing::info!(component = "ipc", socket = %socket_path.display(), "IPC server ready");

    loop {
        let (stream, _) = listener.accept().await?;
        tracing::info!(component = "ipc", "client connected");
        let discovery = Arc::clone(&discovery);
        let sessions = Arc::clone(&sessions);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, discovery, sessions).await {
                tracing::warn!(component = "ipc", error = %error, "client connection ended");
            }
        });
    }
}

async fn handle_connection(
    stream: UnixStream,
    discovery: Arc<DiscoveryService>,
    sessions: Arc<SessionManager>,
) -> io::Result<()> {
    let (read_half, write_half) = stream.into_split();
    let writer = Arc::new(Mutex::new(write_half));
    let streams: ActiveStreams = Arc::new(Mutex::new(HashMap::new()));
    let mut stream_tasks = JoinSet::new();
    let mut lines = BufReader::new(read_half).lines();

    while let Some(line) = lines.next_line().await? {
        let request = match serde_json::from_str::<Request>(&line) {
            Ok(request) => request,
            Err(error) => {
                write_value(
                    &writer,
                    &error_response(None, "malformed_json", format!("Invalid request: {error}")),
                )
                .await?;
                continue;
            }
        };

        if !ProtocolVersion::CURRENT.is_compatible_with(request.version) {
            write_value(
                &writer,
                &error_response(
                    Some(&request.id),
                    "incompatible_version",
                    format!(
                        "Protocol major {} is not supported; expected {}",
                        request.version.major,
                        ProtocolVersion::CURRENT.major
                    ),
                ),
            )
            .await?;
            continue;
        }

        match request.method.as_str() {
            "core.hello" => {
                write_value(
                    &writer,
                    &success_response(
                        &request.id,
                        json!({
                            "name": "vela-core",
                            "core_version": env!("CARGO_PKG_VERSION"),
                            "protocol_version": ProtocolVersion::CURRENT,
                        }),
                    ),
                )
                .await?;
            }
            "core.health" => {
                write_value(
                    &writer,
                    &success_response(&request.id, json!({ "status": "healthy" })),
                )
                .await?;
            }
            "agents.list" => {
                let snapshot = discovery.snapshot().await;
                write_value(&writer, &success_response(&request.id, json!(snapshot))).await?;
            }
            "agents.refresh" => {
                let snapshot = discovery.refresh().await;
                write_value(&writer, &success_response(&request.id, json!(snapshot))).await?;
            }
            "session.create" => {
                let Some(agent_id) = request.params.get("agent_id").and_then(Value::as_str) else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "agent_id is required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let cwd = request
                    .params
                    .get("cwd")
                    .and_then(Value::as_str)
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
                    });
                let cwd = match std::fs::canonicalize(&cwd) {
                    Ok(path) if path.is_dir() => path,
                    _ => {
                        write_value(
                            &writer,
                            &error_response(
                                Some(&request.id),
                                "invalid_cwd",
                                format!("Not a directory: {}", cwd.display()),
                            ),
                        )
                        .await?;
                        continue;
                    }
                };
                let spec = match discovery.launch_spec(agent_id).await {
                    Ok(spec) => spec,
                    Err(message) => {
                        write_value(
                            &writer,
                            &error_response(Some(&request.id), "agent_not_ready", message),
                        )
                        .await?;
                        continue;
                    }
                };
                match sessions.create(agent_id.to_owned(), spec, cwd).await {
                    Ok(session) => {
                        write_value(&writer, &success_response(&request.id, json!(session))).await?
                    }
                    Err(message) => {
                        write_value(
                            &writer,
                            &error_response(Some(&request.id), "session_create_failed", message),
                        )
                        .await?
                    }
                }
            }
            "session.get" => {
                let Some(session_id) = request.params.get("session_id").and_then(Value::as_str)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "session_id is required",
                        ),
                    )
                    .await?;
                    continue;
                };
                match sessions.descriptor(session_id).await {
                    Some(session) => {
                        write_value(&writer, &success_response(&request.id, json!(session))).await?
                    }
                    None => {
                        write_value(
                            &writer,
                            &error_response(
                                Some(&request.id),
                                "session_not_found",
                                format!("Unknown session: {session_id}"),
                            ),
                        )
                        .await?
                    }
                }
            }
            "session.prompt" => {
                let Some(session_id) = request.params.get("session_id").and_then(Value::as_str)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "session_id is required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let Some(text) = request
                    .params
                    .get("text")
                    .and_then(Value::as_str)
                    .filter(|text| !text.trim().is_empty())
                else {
                    write_value(
                        &writer,
                        &error_response(Some(&request.id), "invalid_params", "text is required"),
                    )
                    .await?;
                    continue;
                };
                match sessions
                    .prompt(session_id, request.id.clone(), text.to_owned())
                    .await
                {
                    Ok((accepted, mut receiver)) => {
                        write_value(
                            &writer,
                            &success_response(
                                &request.id,
                                json!({
                                    "session_id": session_id,
                                    "run_id": accepted.run_id,
                                    "acp_request_id": accepted.acp_request_id,
                                }),
                            ),
                        )
                        .await?;
                        let task_writer = Arc::clone(&writer);
                        stream_tasks.spawn(async move {
                            while let Ok(agent_event) = receiver.recv().await {
                                let terminal = agent_event.payload.is_terminal();
                                if write_value(
                                    &task_writer,
                                    &event("agent.event", json!(agent_event)),
                                )
                                .await
                                .is_err()
                                    || terminal
                                {
                                    break;
                                }
                            }
                        });
                    }
                    Err(message) => {
                        write_value(
                            &writer,
                            &error_response(Some(&request.id), "prompt_failed", message),
                        )
                        .await?
                    }
                }
            }
            "session.cancel" => {
                let session_id = request.params.get("session_id").and_then(Value::as_str);
                let run_id = request.params.get("run_id").and_then(Value::as_str);
                let (Some(session_id), Some(run_id)) = (session_id, run_id) else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "session_id and run_id are required",
                        ),
                    )
                    .await?;
                    continue;
                };
                match sessions.cancel(session_id, run_id).await {
                    Ok(cancel_requested) => {
                        write_value(
                            &writer,
                            &success_response(
                                &request.id,
                                json!({ "cancel_requested": cancel_requested }),
                            ),
                        )
                        .await?
                    }
                    Err(message) => {
                        write_value(
                            &writer,
                            &error_response(Some(&request.id), "cancel_failed", message),
                        )
                        .await?
                    }
                }
            }
            "permissions.pending" => {
                let session_id = request.params.get("session_id").and_then(Value::as_str);
                let permissions = sessions.pending_permissions(session_id).await;
                write_value(
                    &writer,
                    &success_response(&request.id, json!({ "permissions": permissions })),
                )
                .await?;
            }
            "permissions.history" => {
                let session_id = request.params.get("session_id").and_then(Value::as_str);
                let records = sessions.permission_history(session_id).await;
                write_value(
                    &writer,
                    &success_response(&request.id, json!({ "records": records })),
                )
                .await?;
            }
            "permission.resolve" => {
                let permission_id = request.params.get("permission_id").and_then(Value::as_str);
                let session_id = request.params.get("session_id").and_then(Value::as_str);
                let run_id = request.params.get("run_id").and_then(Value::as_str);
                let decision = request.params.get("decision").and_then(Value::as_str);
                let decision = match decision {
                    Some("allow_once") => Some(PermissionDecision::AllowOnce),
                    Some("allow_session") => Some(PermissionDecision::AllowSession),
                    Some("deny") => Some(PermissionDecision::Deny),
                    _ => None,
                };
                let (Some(permission_id), Some(session_id), Some(run_id), Some(decision)) =
                    (permission_id, session_id, run_id, decision)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "permission_id, session_id, run_id, and a valid decision are required",
                        ),
                    )
                    .await?;
                    continue;
                };
                match sessions
                    .decide_permission(permission_id, session_id, run_id, decision)
                    .await
                {
                    Ok(record) => {
                        write_value(&writer, &success_response(&request.id, json!(record))).await?
                    }
                    Err(message) => {
                        write_value(
                            &writer,
                            &error_response(
                                Some(&request.id),
                                "permission_resolution_failed",
                                message,
                            ),
                        )
                        .await?
                    }
                }
            }
            "stream.start" => {
                let count = request
                    .params
                    .get("count")
                    .and_then(Value::as_u64)
                    .unwrap_or(20)
                    .clamp(1, 100);
                let interval_ms = request
                    .params
                    .get("interval_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or(150)
                    .clamp(1, 5_000);

                let (cancel_tx, cancel_rx) = watch::channel(false);
                let mut active = streams.lock().await;
                if active.contains_key(&request.id) {
                    drop(active);
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "duplicate_request",
                            "A stream already uses this request ID",
                        ),
                    )
                    .await?;
                    continue;
                }
                active.insert(request.id.clone(), cancel_tx);
                drop(active);

                write_value(
                    &writer,
                    &success_response(&request.id, json!({ "accepted": true, "count": count })),
                )
                .await?;

                let request_id = request.id;
                let task_writer = Arc::clone(&writer);
                let task_streams = Arc::clone(&streams);
                stream_tasks.spawn(async move {
                    run_stream(
                        request_id,
                        count,
                        Duration::from_millis(interval_ms),
                        cancel_rx,
                        task_writer,
                        task_streams,
                    )
                    .await;
                });
            }
            "stream.cancel" => {
                let Some(target_id) = request
                    .params
                    .get("target_request_id")
                    .and_then(Value::as_str)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "target_request_id is required",
                        ),
                    )
                    .await?;
                    continue;
                };

                let cancel = streams.lock().await.get(target_id).cloned();
                write_value(
                    &writer,
                    &success_response(&request.id, json!({ "cancel_requested": cancel.is_some() })),
                )
                .await?;
                if let Some(cancel) = cancel {
                    let _ = cancel.send(true);
                }
            }
            _ => {
                write_value(
                    &writer,
                    &error_response(
                        Some(&request.id),
                        "method_not_found",
                        format!("Unknown method: {}", request.method),
                    ),
                )
                .await?;
            }
        }
    }

    for cancel in streams.lock().await.drain().map(|(_, cancel)| cancel) {
        let _ = cancel.send(true);
    }
    stream_tasks.abort_all();
    tracing::info!(component = "ipc", "client disconnected");
    Ok(())
}

async fn run_stream(
    request_id: String,
    count: u64,
    interval: Duration,
    mut cancel: watch::Receiver<bool>,
    writer: SharedWriter,
    streams: ActiveStreams,
) {
    let mut terminal_event = "stream.completed";
    let mut emitted = 0;

    for sequence in 1..=count {
        tokio::select! {
            biased;
            changed = cancel.changed() => {
                if changed.is_ok() && *cancel.borrow() {
                    terminal_event = "stream.cancelled";
                    break;
                }
            }
            _ = tokio::time::sleep(interval) => {
                let event = event(
                    "stream.chunk",
                    json!({
                        "request_id": request_id,
                        "sequence": sequence,
                        "text": format!("Deterministic chunk {sequence}"),
                    }),
                );
                if write_value(&writer, &event).await.is_err() {
                    streams.lock().await.remove(&request_id);
                    return;
                }
                emitted = sequence;
            }
        }
    }

    let terminal = event(
        terminal_event,
        json!({ "request_id": request_id, "emitted": emitted }),
    );
    let _ = write_value(&writer, &terminal).await;
    streams.lock().await.remove(&request_id);
    tracing::info!(
        component = "ipc",
        request_id,
        terminal_event,
        emitted,
        "stream ended"
    );
}

async fn write_value(writer: &SharedWriter, value: &Value) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    writer.lock().await.write_all(&bytes).await
}

fn envelope() -> Value {
    json!({ "version": ProtocolVersion::CURRENT })
}

fn success_response(id: &str, result: Value) -> Value {
    let mut response = envelope();
    response["id"] = json!(id);
    response["result"] = result;
    response
}

fn error_response(id: Option<&str>, code: &str, message: impl Into<String>) -> Value {
    let mut response = envelope();
    response["id"] = id.map_or(Value::Null, |id| json!(id));
    response["error"] = json!({ "code": code, "message": message.into() });
    response
}

fn event(name: &str, data: Value) -> Value {
    let mut event = envelope();
    event["event"] = json!(name);
    event["data"] = data;
    event
}
