use std::{
    collections::HashMap,
    io,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use acp_runtime::SessionManager;
use capture_engine::{CaptureEngine, CaptureError};
use domain::{
    CaptureIntent, CaptureSource, PermissionDecision, ProtocolVersion, WorkspaceProvenance,
};
use harness_discovery::{DiscoveryOptions, DiscoveryService};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixListener, UnixStream},
    sync::{watch, Mutex},
    task::JoinSet,
};
use workspace_engine::{WorkspaceError, WorkspaceService};

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
    serve_with_all_services(
        socket_path,
        discovery,
        sessions,
        Arc::new(WorkspaceService::default()),
    )
    .await
}

pub async fn serve_with_all_services(
    socket_path: impl AsRef<Path>,
    discovery: Arc<DiscoveryService>,
    sessions: Arc<SessionManager>,
    workspaces: Arc<WorkspaceService>,
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
        let workspaces = Arc::clone(&workspaces);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream, discovery, sessions, workspaces).await {
                tracing::warn!(component = "ipc", error = %error, "client connection ended");
            }
        });
    }
}

async fn handle_connection(
    stream: UnixStream,
    discovery: Arc<DiscoveryService>,
    sessions: Arc<SessionManager>,
    workspaces: Arc<WorkspaceService>,
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
            "workspace.open" => {
                let Some(root) = request.params.get("root").and_then(Value::as_str) else {
                    write_value(
                        &writer,
                        &error_response(Some(&request.id), "invalid_params", "root is required"),
                    )
                    .await?;
                    continue;
                };
                write_workspace_result(
                    &writer,
                    &request.id,
                    workspaces.open(root).await.map(|snapshot| json!(snapshot)),
                )
                .await?;
            }
            "workspace.status" => {
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace.snapshot().map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.refresh" => {
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace
                        .reconcile(WorkspaceProvenance::ExternalFilesystem, Some(&request.id))
                        .and_then(|_| workspace.snapshot())
                        .map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.write" => {
                let path = request.params.get("path").and_then(Value::as_str);
                let content = request.params.get("content").and_then(Value::as_str);
                let provenance = request
                    .params
                    .get("provenance")
                    .and_then(Value::as_str)
                    .map(parse_workspace_provenance)
                    .transpose();
                let (Some(path), Some(content), Ok(provenance)) = (path, content, provenance)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "path, content, and a valid provenance are required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace
                        .write_file(
                            path,
                            content,
                            provenance.unwrap_or(WorkspaceProvenance::User),
                            Some(&request.id),
                        )
                        .map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.reference.add" => {
                let Some(path) = request.params.get("path").and_then(Value::as_str) else {
                    write_value(
                        &writer,
                        &error_response(Some(&request.id), "invalid_params", "path is required"),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace
                        .add_reference(path, Some(&request.id))
                        .map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.reference.remove" => {
                let Some(reference_id) = request.params.get("reference_id").and_then(Value::as_str)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "reference_id is required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace
                        .remove_reference(reference_id, Some(&request.id))
                        .map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.events" => {
                let limit = request
                    .params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(50);
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace
                        .events(limit)
                        .map(|events| json!({ "events": events })),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.rebuild" => {
                let result = match workspaces.active().await {
                    Ok(workspace) => workspace.rebuild_index().map(|snapshot| json!(snapshot)),
                    Err(error) => Err(error),
                };
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "workspace.context" => {
                let scope = request.params.get("scope").and_then(Value::as_str);
                let result = match (workspaces.active().await, scope) {
                    (Ok(workspace), Some("status")) => workspace.context_status(),
                    (Ok(workspace), Some("workspace_path")) => request
                        .params
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| WorkspaceError("path is required".to_owned()))
                        .and_then(|path| workspace.context_workspace_path(path)),
                    (Ok(workspace), Some("reference_path")) => {
                        let reference_id = request
                            .params
                            .get("reference_id")
                            .and_then(Value::as_str)
                            .ok_or_else(|| WorkspaceError("reference_id is required".to_owned()));
                        let path = request
                            .params
                            .get("path")
                            .and_then(Value::as_str)
                            .ok_or_else(|| WorkspaceError("path is required".to_owned()));
                        reference_id.and_then(|reference_id| {
                            path.and_then(|path| {
                                workspace.context_reference_path(reference_id, path)
                            })
                        })
                    }
                    (Ok(_), _) => Err(WorkspaceError(
                        "scope must be status, workspace_path, or reference_path".to_owned(),
                    )),
                    (Err(error), _) => Err(error),
                }
                .map(|context| json!(context));
                write_workspace_result(&writer, &request.id, result).await?;
            }
            "capture.create" => {
                let source = request
                    .params
                    .get("source")
                    .and_then(Value::as_str)
                    .map(parse_capture_source)
                    .transpose();
                let raw_text = request.params.get("raw_text").and_then(Value::as_str);
                let intent = request
                    .params
                    .get("intent")
                    .and_then(Value::as_str)
                    .map(parse_capture_intent)
                    .transpose();
                let started_at_ms = request
                    .params
                    .get("started_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms);
                let (Ok(Some(source)), Some(raw_text), Ok(intent)) = (source, raw_text, intent)
                else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "source, raw_text, and a valid optional intent are required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => CaptureEngine::create(
                        &workspace,
                        source,
                        raw_text,
                        intent,
                        started_at_ms,
                        Some(&request.id),
                    )
                    .map(|record| json!(record)),
                    Err(error) => Err(CaptureError(error.to_string())),
                };
                write_capture_result(&writer, &request.id, result).await?;
            }
            "capture.abandon" => {
                let source = request
                    .params
                    .get("source")
                    .and_then(Value::as_str)
                    .map(parse_capture_source)
                    .transpose();
                let raw_text = request
                    .params
                    .get("raw_text")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let started_at_ms = request
                    .params
                    .get("started_at_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_else(now_ms);
                let Ok(Some(source)) = source else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "a valid source is required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => CaptureEngine::abandon(
                        &workspace,
                        source,
                        raw_text,
                        started_at_ms,
                        Some(&request.id),
                    )
                    .map(|record| json!(record)),
                    Err(error) => Err(CaptureError(error.to_string())),
                };
                write_capture_result(&writer, &request.id, result).await?;
            }
            "capture.correct" => {
                let capture_id = request.params.get("capture_id").and_then(Value::as_str);
                let intent = request
                    .params
                    .get("intent")
                    .and_then(Value::as_str)
                    .map(parse_capture_intent)
                    .transpose();
                let (Some(capture_id), Ok(Some(intent))) = (capture_id, intent) else {
                    write_value(
                        &writer,
                        &error_response(
                            Some(&request.id),
                            "invalid_params",
                            "capture_id and a valid intent are required",
                        ),
                    )
                    .await?;
                    continue;
                };
                let result = match workspaces.active().await {
                    Ok(workspace) => {
                        CaptureEngine::correct(&workspace, capture_id, intent, Some(&request.id))
                            .map(|record| json!(record))
                    }
                    Err(error) => Err(CaptureError(error.to_string())),
                };
                write_capture_result(&writer, &request.id, result).await?;
            }
            "capture.list" => {
                let limit = request
                    .params
                    .get("limit")
                    .and_then(Value::as_u64)
                    .unwrap_or(50) as usize;
                let result = match workspaces.active().await {
                    Ok(workspace) => CaptureEngine::list(&workspace, limit)
                        .map(|captures| json!({ "captures": captures })),
                    Err(error) => Err(CaptureError(error.to_string())),
                };
                write_capture_result(&writer, &request.id, result).await?;
            }
            "capture.metrics" => {
                let since_ms = request
                    .params
                    .get("since_ms")
                    .and_then(Value::as_u64)
                    .unwrap_or_default();
                let result = match workspaces.active().await {
                    Ok(workspace) => {
                        CaptureEngine::metrics(&workspace, since_ms).map(|metrics| json!(metrics))
                    }
                    Err(error) => Err(CaptureError(error.to_string())),
                };
                write_capture_result(&writer, &request.id, result).await?;
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

async fn write_workspace_result(
    writer: &SharedWriter,
    request_id: &str,
    result: Result<Value, WorkspaceError>,
) -> io::Result<()> {
    let response = match result {
        Ok(value) => success_response(request_id, value),
        Err(error) => error_response(Some(request_id), "workspace_error", error.to_string()),
    };
    write_value(writer, &response).await
}

fn parse_workspace_provenance(value: &str) -> Result<WorkspaceProvenance, WorkspaceError> {
    match value {
        "user" => Ok(WorkspaceProvenance::User),
        "agent" => Ok(WorkspaceProvenance::Agent),
        "tool" => Ok(WorkspaceProvenance::Tool),
        "scheduler" => Ok(WorkspaceProvenance::Scheduler),
        _ => Err(WorkspaceError(format!(
            "Unsupported write provenance: {value}"
        ))),
    }
}

async fn write_capture_result(
    writer: &SharedWriter,
    request_id: &str,
    result: Result<Value, CaptureError>,
) -> io::Result<()> {
    let response = match result {
        Ok(value) => success_response(request_id, value),
        Err(error) => error_response(Some(request_id), "capture_error", error.to_string()),
    };
    write_value(writer, &response).await
}

fn parse_capture_source(value: &str) -> Result<CaptureSource, CaptureError> {
    match value {
        "text" => Ok(CaptureSource::Text),
        "speech" => Ok(CaptureSource::Speech),
        _ => Err(CaptureError(format!("Unsupported capture source: {value}"))),
    }
}

fn parse_capture_intent(value: &str) -> Result<CaptureIntent, CaptureError> {
    match value {
        "note" => Ok(CaptureIntent::Note),
        "idea" => Ok(CaptureIntent::Idea),
        "todo" => Ok(CaptureIntent::Todo),
        "work_update" => Ok(CaptureIntent::WorkUpdate),
        "unknown" => Ok(CaptureIntent::Unknown),
        _ => Err(CaptureError(format!("Unsupported capture intent: {value}"))),
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
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
