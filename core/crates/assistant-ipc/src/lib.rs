use std::{collections::HashMap, io, path::Path, sync::Arc, time::Duration};

use domain::ProtocolVersion;
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
        tokio::spawn(async move {
            if let Err(error) = handle_connection(stream).await {
                tracing::warn!(component = "ipc", error = %error, "client connection ended");
            }
        });
    }
}

async fn handle_connection(stream: UnixStream) -> io::Result<()> {
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
