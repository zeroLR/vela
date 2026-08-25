use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use acp_runtime::{AcpLaunchSpec, SessionManager};
use domain::{
    AgentEvent, AgentEventPayload, PermissionCategory, PermissionDecision, PermissionRequest,
    PermissionResolutionSource, PermissionResolutionStatus,
};
use tokio::sync::broadcast;

fn spec(scenario: &str, kind: &str) -> AcpLaunchSpec {
    AcpLaunchSpec {
        executable: PathBuf::from(env!("CARGO_BIN_EXE_fake-acp-harness")),
        arguments: vec![
            "--scenario".to_owned(),
            scenario.to_owned(),
            "--permission-kind".to_owned(),
            kind.to_owned(),
        ],
        environment: BTreeMap::new(),
        enforced_session_mode: "safe".to_owned(),
    }
}

async fn next_event(receiver: &mut broadcast::Receiver<AgentEvent>, run_id: &str) -> AgentEvent {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(2), receiver.recv())
            .await
            .expect("timed out waiting for permission event")
            .expect("permission event channel closed");
        if event.run_id == run_id {
            return event;
        }
    }
}

async fn next_request(
    receiver: &mut broadcast::Receiver<AgentEvent>,
    run_id: &str,
) -> PermissionRequest {
    loop {
        if let AgentEventPayload::PermissionRequested { request } =
            next_event(receiver, run_id).await.payload
        {
            return request;
        }
    }
}

async fn collect_terminal(
    receiver: &mut broadcast::Receiver<AgentEvent>,
    run_id: &str,
) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    loop {
        let event = next_event(receiver, run_id).await;
        let terminal = event.payload.is_terminal();
        events.push(event);
        if terminal {
            return events;
        }
    }
}

#[tokio::test]
async fn fake_harness_covers_permission_categories_decisions_scopes_and_cleanup() {
    let manager = SessionManager::with_permission_timeout(
        Duration::from_secs(1),
        Duration::from_secs(2),
        Duration::from_millis(250),
    );
    let cwd = std::env::current_dir().unwrap();

    for (kind, expected) in [
        ("read", PermissionCategory::FilesystemRead),
        ("edit", PermissionCategory::FilesystemWrite),
        ("execute", PermissionCategory::ShellExecute),
        ("fetch", PermissionCategory::NetworkOpenUrl),
        ("mcp", PermissionCategory::McpInvoke),
        ("other", PermissionCategory::Other),
    ] {
        let session = manager
            .create(
                format!("fake-{kind}"),
                spec("permission", kind),
                cwd.clone(),
            )
            .await
            .unwrap();
        let (accepted, mut receiver) = manager
            .prompt(&session.id, format!("ipc-{kind}"), "request".to_owned())
            .await
            .unwrap();
        let request = next_request(&mut receiver, &accepted.run_id).await;
        assert_eq!(request.category, expected);
        assert!(!request.title.is_empty());
        manager
            .decide_permission(
                &request.id,
                &request.session_id,
                &request.run_id,
                PermissionDecision::AllowOnce,
            )
            .await
            .unwrap();
        let events = collect_terminal(&mut receiver, &accepted.run_id).await;
        assert!(events.iter().any(|event| matches!(
            event.payload,
            AgentEventPayload::PermissionResolved { ref record }
                if record.status == PermissionResolutionStatus::Allowed
        )));
        assert!(matches!(
            events.last().unwrap().payload,
            AgentEventPayload::Completed { .. }
        ));
    }

    let denied = manager
        .create(
            "fake-deny".to_owned(),
            spec("permission", "edit"),
            cwd.clone(),
        )
        .await
        .unwrap();
    let (accepted, mut receiver) = manager
        .prompt(&denied.id, "ipc-deny".to_owned(), "deny".to_owned())
        .await
        .unwrap();
    let request = next_request(&mut receiver, &accepted.run_id).await;
    let record = manager
        .decide_permission(
            &request.id,
            &request.session_id,
            &request.run_id,
            PermissionDecision::Deny,
        )
        .await
        .unwrap();
    assert_eq!(record.status, PermissionResolutionStatus::Denied);
    assert!(matches!(
        collect_terminal(&mut receiver, &accepted.run_id)
            .await
            .last()
            .unwrap()
            .payload,
        AgentEventPayload::Completed { .. }
    ));

    let granted = manager
        .create(
            "fake-session-grant".to_owned(),
            spec("permission", "edit"),
            cwd.clone(),
        )
        .await
        .unwrap();
    let (first, mut receiver) = manager
        .prompt(&granted.id, "ipc-grant-1".to_owned(), "grant".to_owned())
        .await
        .unwrap();
    let request = next_request(&mut receiver, &first.run_id).await;
    manager
        .decide_permission(
            &request.id,
            &request.session_id,
            &request.run_id,
            PermissionDecision::AllowSession,
        )
        .await
        .unwrap();
    collect_terminal(&mut receiver, &first.run_id).await;

    let (second, mut receiver) = manager
        .prompt(&granted.id, "ipc-grant-2".to_owned(), "reuse".to_owned())
        .await
        .unwrap();
    let events = collect_terminal(&mut receiver, &second.run_id).await;
    assert!(events.iter().any(|event| matches!(
        event.payload,
        AgentEventPayload::PermissionResolved { ref record }
            if record.source == PermissionResolutionSource::SessionGrant
    )));
    assert!(manager
        .pending_permissions(Some(&granted.id))
        .await
        .is_empty());

    let concurrent_a = manager
        .create(
            "fake-concurrent-a".to_owned(),
            spec("permission", "edit"),
            cwd.clone(),
        )
        .await
        .unwrap();
    let concurrent_b = manager
        .create(
            "fake-concurrent-b".to_owned(),
            spec("permission", "execute"),
            cwd.clone(),
        )
        .await
        .unwrap();
    let (accepted_a, mut receiver_a) = manager
        .prompt(
            &concurrent_a.id,
            "ipc-concurrent-a".to_owned(),
            "first".to_owned(),
        )
        .await
        .unwrap();
    let (accepted_b, mut receiver_b) = manager
        .prompt(
            &concurrent_b.id,
            "ipc-concurrent-b".to_owned(),
            "second".to_owned(),
        )
        .await
        .unwrap();
    let first = next_request(&mut receiver_a, &accepted_a.run_id).await;
    let second = next_request(&mut receiver_b, &accepted_b.run_id).await;
    assert_ne!(first.id, second.id);
    assert_eq!(manager.pending_permissions(None).await.len(), 2);
    manager
        .decide_permission(
            &first.id,
            &first.session_id,
            &first.run_id,
            PermissionDecision::AllowOnce,
        )
        .await
        .unwrap();
    manager
        .decide_permission(
            &second.id,
            &second.session_id,
            &second.run_id,
            PermissionDecision::Deny,
        )
        .await
        .unwrap();
    collect_terminal(&mut receiver_a, &accepted_a.run_id).await;
    collect_terminal(&mut receiver_b, &accepted_b.run_id).await;
    assert!(manager.pending_permissions(None).await.is_empty());

    let cancelled = manager
        .create(
            "fake-permission-cancel".to_owned(),
            spec("permission", "execute"),
            cwd,
        )
        .await
        .unwrap();
    let (accepted, mut receiver) = manager
        .prompt(
            &cancelled.id,
            "ipc-permission-cancel".to_owned(),
            "cancel".to_owned(),
        )
        .await
        .unwrap();
    next_request(&mut receiver, &accepted.run_id).await;
    assert!(manager
        .cancel(&cancelled.id, &accepted.run_id)
        .await
        .unwrap());
    let events = collect_terminal(&mut receiver, &accepted.run_id).await;
    assert!(events.iter().any(|event| matches!(
        event.payload,
        AgentEventPayload::PermissionResolved { ref record }
            if record.status == PermissionResolutionStatus::Cancelled
    )));
    assert!(manager
        .pending_permissions(Some(&cancelled.id))
        .await
        .is_empty());
}

#[tokio::test]
async fn unresolved_permission_times_out_to_safe_cancellation() {
    let manager = SessionManager::with_permission_timeout(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(30),
    );
    let session = manager
        .create(
            "fake-timeout".to_owned(),
            spec("permission", "execute"),
            std::env::current_dir().unwrap(),
        )
        .await
        .unwrap();
    let (accepted, mut receiver) = manager
        .prompt(&session.id, "ipc-timeout".to_owned(), "wait".to_owned())
        .await
        .unwrap();
    let events = collect_terminal(&mut receiver, &accepted.run_id).await;
    assert!(events.iter().any(|event| matches!(
        event.payload,
        AgentEventPayload::PermissionResolved { ref record }
            if record.status == PermissionResolutionStatus::TimedOut
    )));
    assert!(matches!(
        events.last().unwrap().payload,
        AgentEventPayload::Cancelled
    ));
    assert_eq!(manager.permission_history(Some(&session.id)).await.len(), 1);
}
