use std::{path::PathBuf, time::Duration};

use acp_runtime::{AcpLaunchSpec, SessionManager};
use domain::{AgentEvent, AgentEventPayload, SessionState};

fn spec(scenario: &str) -> AcpLaunchSpec {
    AcpLaunchSpec {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_fake-acp-harness")),
        arguments: vec!["--scenario".to_owned(), scenario.to_owned()],
    }
}

async fn collect_terminal(
    mut receiver: tokio::sync::broadcast::Receiver<AgentEvent>,
    run_id: &str,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("timed out waiting for session event")
            .expect("session event channel closed");
        if event.run_id != run_id {
            continue;
        }
        let terminal = event.payload.is_terminal();
        events.push(event);
        if terminal {
            return events;
        }
    }
}

#[tokio::test]
async fn fake_harness_covers_session_lifecycle_failures_and_recovery() {
    let manager = SessionManager::new(Duration::from_secs(1), Duration::from_millis(150));
    let cwd = std::env::current_dir().unwrap();

    let success = manager
        .create("fake".to_owned(), spec("ready"), cwd.clone())
        .await
        .unwrap();
    assert_eq!(success.state, SessionState::Ready);
    assert!(success.process_id > 0);
    let (accepted, receiver) = manager
        .prompt(&success.id, "ipc-success".to_owned(), "hello".to_owned())
        .await
        .unwrap();
    assert!(!accepted.acp_request_id.is_empty());
    let events = collect_terminal(receiver, &accepted.run_id).await;
    assert_eq!(events.first().unwrap().sequence, 1);
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, AgentEventPayload::TextDelta { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, AgentEventPayload::PlanUpdated { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, AgentEventPayload::ToolStarted { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, AgentEventPayload::UsageUpdated { .. })));
    assert!(matches!(
        events.last().unwrap().payload,
        AgentEventPayload::Completed { .. }
    ));

    let cancelled = manager
        .create("fake-cancel".to_owned(), spec("cancel"), cwd.clone())
        .await
        .unwrap();
    let (accepted, receiver) = manager
        .prompt(&cancelled.id, "ipc-cancel".to_owned(), "wait".to_owned())
        .await
        .unwrap();
    assert!(manager
        .cancel(&cancelled.id, &accepted.run_id)
        .await
        .unwrap());
    let events = collect_terminal(receiver, &accepted.run_id).await;
    assert!(matches!(
        events.last().unwrap().payload,
        AgentEventPayload::Cancelled
    ));

    let permission = manager
        .create(
            "fake-permission".to_owned(),
            spec("permission"),
            cwd.clone(),
        )
        .await
        .unwrap();
    let (accepted, receiver) = manager
        .prompt(
            &permission.id,
            "ipc-permission".to_owned(),
            "write".to_owned(),
        )
        .await
        .unwrap();
    let events = collect_terminal(receiver, &accepted.run_id).await;
    assert!(events
        .iter()
        .any(|event| matches!(event.payload, AgentEventPayload::PermissionRequested { .. })));
    assert!(matches!(
        events.last().unwrap().payload,
        AgentEventPayload::Cancelled
    ));

    for scenario in ["prompt-timeout", "unexpected-exit", "malformed-event"] {
        let failed = manager
            .create(format!("fake-{scenario}"), spec(scenario), cwd.clone())
            .await
            .unwrap();
        let (accepted, receiver) = manager
            .prompt(&failed.id, format!("ipc-{scenario}"), "fail".to_owned())
            .await
            .unwrap();
        let events = collect_terminal(receiver, &accepted.run_id).await;
        assert!(matches!(
            events.last().unwrap().payload,
            AgentEventPayload::Failed { .. }
        ));
    }

    let recovered = manager
        .create("fake-recovered".to_owned(), spec("ready"), cwd)
        .await
        .unwrap();
    let (accepted, receiver) = manager
        .prompt(
            &recovered.id,
            "ipc-recovered".to_owned(),
            "again".to_owned(),
        )
        .await
        .unwrap();
    assert!(matches!(
        collect_terminal(receiver, &accepted.run_id)
            .await
            .last()
            .unwrap()
            .payload,
        AgentEventPayload::Completed { .. }
    ));
}
