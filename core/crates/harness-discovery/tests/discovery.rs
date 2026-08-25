use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use domain::AgentStatus;
use harness_discovery::{DiscoveryOptions, DiscoveryService};
use serde_json::json;

fn temporary_directory() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("vela-discovery-{}-{nonce}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    path
}

fn agent<'a>(snapshot: &'a domain::AgentRegistrySnapshot, id: &str) -> &'a domain::AgentDescriptor {
    snapshot.agents.iter().find(|agent| agent.id == id).unwrap()
}

#[tokio::test]
async fn refresh_normalizes_acp_results_and_failure_states() {
    let root = temporary_directory();
    let config_path = root.join("harnesses.json");
    let pid_path = root.join("timeout.pid");
    let executable = env!("CARGO_BIN_EXE_fake-acp-harness");
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&json!({
            "harnesses": [
                harness(
                    "fake-ready",
                    executable,
                    &[
                        "--scenario",
                        "ready",
                        "--require-env",
                        "VELA_TEST_ENFORCEMENT=enabled",
                    ],
                ),
                harness("fake-auth", executable, &["--scenario", "unauthenticated"]),
                harness("fake-version", executable, &["--scenario", "incompatible"]),
                harness("fake-invalid", executable, &["--scenario", "invalid"]),
                harness(
                    "fake-timeout",
                    executable,
                    &["--scenario", "timeout", "--pid-file", pid_path.to_str().unwrap()],
                )
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let service = DiscoveryService::new(DiscoveryOptions {
        path: None,
        known_directories: Vec::new(),
        config_path: Some(config_path),
        version_timeout: Duration::from_secs(1),
        initialize_timeout: Duration::from_millis(250),
    });
    let snapshot = service.refresh().await;

    assert_eq!(snapshot.generation, 1);
    assert_eq!(agent(&snapshot, "claude").status, AgentStatus::Unavailable);
    assert_eq!(agent(&snapshot, "codex").status, AgentStatus::Unavailable);
    let ready = agent(&snapshot, "fake-ready");
    assert_eq!(ready.status, AgentStatus::Ready);
    assert_eq!(ready.version.as_deref(), Some("fake-acp-harness 0.1.0"));
    assert_eq!(ready.protocol_version.as_deref(), Some("1"));
    assert_eq!(ready.enforced_session_mode, "safe");
    assert!(ready.capabilities.iter().any(|item| item == "prompt.image"));
    assert!(ready.capabilities.iter().any(|item| item == "session.list"));
    let launch = service.launch_spec("fake-ready").await.unwrap();
    assert_eq!(launch.enforced_session_mode, "safe");
    assert_eq!(
        launch
            .environment
            .get("VELA_TEST_ENFORCEMENT")
            .map(String::as_str),
        Some("enabled")
    );
    assert_eq!(
        agent(&snapshot, "fake-auth").status,
        AgentStatus::Unauthenticated
    );
    assert_eq!(
        agent(&snapshot, "fake-version").status,
        AgentStatus::Incompatible
    );
    assert_eq!(agent(&snapshot, "fake-invalid").status, AgentStatus::Failed);
    assert_eq!(agent(&snapshot, "fake-timeout").status, AgentStatus::Failed);

    let second = service.refresh().await;
    assert_eq!(second.generation, 2);
    assert_eq!(service.snapshot().await, second);

    assert_process_exits(&pid_path).await;
    fs::remove_dir_all(root).unwrap();
}

fn harness(id: &str, executable: &str, arguments: &[&str]) -> serde_json::Value {
    json!({
        "id": id,
        "display_name": id,
        "command": executable,
        "enforced_session_mode": "safe",
        "launch_environment": {"VELA_TEST_ENFORCEMENT": "enabled"},
        "launch_arguments": arguments
    })
}

async fn assert_process_exits(pid_path: &Path) {
    for _ in 0..20 {
        if let Ok(pid) = fs::read_to_string(pid_path) {
            if !Command::new("/bin/kill")
                .args(["-0", pid.trim()])
                .status()
                .is_ok_and(|status| status.success())
            {
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed-out ACP harness process remained alive");
}
