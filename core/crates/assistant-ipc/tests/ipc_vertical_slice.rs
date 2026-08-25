use std::{
    io,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use harness_discovery::{DiscoveryOptions, DiscoveryService};
use serde_json::{json, Value};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{unix::OwnedWriteHalf, UnixStream},
    task::JoinHandle,
};

struct TestServer {
    path: PathBuf,
    task: JoinHandle<io::Result<()>>,
}

impl TestServer {
    async fn start(name: &str) -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let path = PathBuf::from(format!(
            "/tmp/vela-{name}-{}-{unique}.sock",
            std::process::id()
        ));
        let server_path = path.clone();
        let discovery = Arc::new(DiscoveryService::new(DiscoveryOptions {
            path: None,
            known_directories: Vec::new(),
            config_path: None,
            version_timeout: Duration::from_millis(50),
            initialize_timeout: Duration::from_millis(50),
        }));
        let task = tokio::spawn(async move {
            assistant_ipc::serve_with_discovery(server_path, discovery).await
        });

        for _ in 0..100 {
            if path.exists() {
                return Self { path, task };
            }
            if task.is_finished() {
                panic!("test server exited before readiness: {:?}", task.await);
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        panic!("test server did not create its socket");
    }

    async fn connect(&self) -> (BufReader<tokio::net::unix::OwnedReadHalf>, OwnedWriteHalf) {
        let stream = UnixStream::connect(&self.path).await.unwrap();
        let (read, write) = stream.into_split();
        (BufReader::new(read), write)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.task.abort();
        let _ = std::fs::remove_file(&self.path);
    }
}

fn request(id: &str, method: &str, params: Value) -> Value {
    json!({
        "version": { "major": 1, "minor": 0 },
        "id": id,
        "method": method,
        "params": params,
    })
}

async fn send(write: &mut OwnedWriteHalf, value: &Value) {
    write
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
}

async fn receive(reader: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Value {
    let mut line = String::new();
    tokio::time::timeout(Duration::from_secs(2), reader.read_line(&mut line))
        .await
        .expect("timed out waiting for IPC message")
        .unwrap();
    serde_json::from_str(&line).unwrap()
}

#[tokio::test]
async fn streams_ordered_events_to_completion() {
    let server = TestServer::start("success").await;
    let (mut reader, mut writer) = server.connect().await;

    send(
        &mut writer,
        &request(
            "stream-1",
            "stream.start",
            json!({ "count": 3, "interval_ms": 1 }),
        ),
    )
    .await;
    let accepted = receive(&mut reader).await;
    assert_eq!(accepted["id"], "stream-1");
    assert_eq!(accepted["result"]["accepted"], true);

    for expected_sequence in 1..=3 {
        let event = receive(&mut reader).await;
        assert_eq!(event["event"], "stream.chunk");
        assert_eq!(event["data"]["request_id"], "stream-1");
        assert_eq!(event["data"]["sequence"], expected_sequence);
    }
    let terminal = receive(&mut reader).await;
    assert_eq!(terminal["event"], "stream.completed");
    assert_eq!(terminal["data"]["emitted"], 3);
}

#[tokio::test]
async fn cancellation_yields_one_terminal_event_and_no_later_chunks() {
    let server = TestServer::start("cancel").await;
    let (mut reader, mut writer) = server.connect().await;

    send(
        &mut writer,
        &request(
            "stream-2",
            "stream.start",
            json!({ "count": 20, "interval_ms": 2 }),
        ),
    )
    .await;
    receive(&mut reader).await;
    assert_eq!(receive(&mut reader).await["event"], "stream.chunk");

    send(
        &mut writer,
        &request(
            "cancel-2",
            "stream.cancel",
            json!({ "target_request_id": "stream-2" }),
        ),
    )
    .await;
    assert_eq!(receive(&mut reader).await["id"], "cancel-2");
    let terminal = receive(&mut reader).await;
    assert_eq!(terminal["event"], "stream.cancelled");

    let mut extra = String::new();
    assert!(
        tokio::time::timeout(Duration::from_millis(30), reader.read_line(&mut extra))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_input_returns_an_error_without_breaking_the_connection() {
    let server = TestServer::start("malformed").await;
    let (mut reader, mut writer) = server.connect().await;

    writer.write_all(b"{not-json}\n").await.unwrap();
    let error = receive(&mut reader).await;
    assert_eq!(error["error"]["code"], "malformed_json");

    send(&mut writer, &request("health-1", "core.health", json!({}))).await;
    let health = receive(&mut reader).await;
    assert_eq!(health["result"]["status"], "healthy");
}

#[tokio::test]
async fn agent_registry_can_be_listed_and_explicitly_refreshed() {
    let server = TestServer::start("agents").await;
    let (mut reader, mut writer) = server.connect().await;

    send(&mut writer, &request("agents-1", "agents.list", json!({}))).await;
    let initial = receive(&mut reader).await;
    assert_eq!(initial["result"]["generation"], 0);
    assert_eq!(initial["result"]["agents"], json!([]));

    send(
        &mut writer,
        &request("agents-2", "agents.refresh", json!({})),
    )
    .await;
    let refreshed = receive(&mut reader).await;
    assert_eq!(refreshed["result"]["generation"], 1);
    assert_eq!(refreshed["result"]["agents"][0]["id"], "claude");
    assert_eq!(refreshed["result"]["agents"][0]["status"], "unavailable");

    send(&mut writer, &request("agents-3", "agents.list", json!({}))).await;
    assert_eq!(receive(&mut reader).await["result"], refreshed["result"]);
}

#[tokio::test]
async fn incompatible_major_version_is_rejected() {
    let server = TestServer::start("version").await;
    let (mut reader, mut writer) = server.connect().await;
    let incompatible = json!({
        "version": { "major": 2, "minor": 0 },
        "id": "hello-2",
        "method": "core.hello",
        "params": {},
    });

    send(&mut writer, &incompatible).await;
    assert_eq!(
        receive(&mut reader).await["error"]["code"],
        "incompatible_version"
    );
}

#[tokio::test]
async fn a_client_can_reconnect_after_disconnect() {
    let server = TestServer::start("reconnect").await;
    let (reader, writer) = server.connect().await;
    drop(reader);
    drop(writer);

    let (mut reader, mut writer) = server.connect().await;
    send(&mut writer, &request("health-2", "core.health", json!({}))).await;
    assert_eq!(receive(&mut reader).await["result"]["status"], "healthy");
}
