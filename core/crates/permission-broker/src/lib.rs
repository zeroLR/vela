use std::{
    collections::{HashMap, HashSet},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use domain::{
    PermissionAuditRecord, PermissionCategory, PermissionDecision, PermissionOption,
    PermissionOptionKind, PermissionRequest, PermissionResolutionSource,
    PermissionResolutionStatus,
};
use tokio::sync::{oneshot, Mutex};

#[derive(Debug, Clone)]
pub struct PermissionIntent {
    pub agent_id: String,
    pub session_id: String,
    pub run_id: String,
    pub request_id: String,
    pub tool_call_id: String,
    pub category: PermissionCategory,
    pub title: String,
    pub target: Option<String>,
}

pub struct PermissionTicket {
    pub request: PermissionRequest,
    resolution: TicketResolution,
}

enum TicketResolution {
    Immediate(Box<PermissionAuditRecord>),
    Pending(oneshot::Receiver<PermissionAuditRecord>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct GrantKey {
    session_id: String,
    category: PermissionCategory,
    title: String,
    target: Option<String>,
}

struct PendingPermission {
    request: PermissionRequest,
    response: oneshot::Sender<PermissionAuditRecord>,
}

#[derive(Default)]
struct BrokerState {
    pending: HashMap<String, PendingPermission>,
    session_grants: HashSet<GrantKey>,
    history: Vec<PermissionAuditRecord>,
}

pub struct PermissionBroker {
    state: Mutex<BrokerState>,
    next_id: AtomicU64,
    timeout: Duration,
}

impl PermissionBroker {
    pub fn new(timeout: Duration) -> Self {
        Self {
            state: Mutex::new(BrokerState::default()),
            next_id: AtomicU64::new(1),
            timeout,
        }
    }

    pub async fn register(
        &self,
        intent: PermissionIntent,
        options: Vec<PermissionOption>,
    ) -> PermissionTicket {
        let created_at_ms = now_ms();
        let request = PermissionRequest {
            id: format!(
                "permission-{}-{}",
                std::process::id(),
                self.next_id.fetch_add(1, Ordering::Relaxed)
            ),
            agent_id: intent.agent_id,
            session_id: intent.session_id,
            run_id: intent.run_id,
            request_id: intent.request_id,
            tool_call_id: intent.tool_call_id,
            category: intent.category,
            title: intent.title,
            target: intent.target,
            options,
            created_at_ms,
            expires_at_ms: created_at_ms.saturating_add(self.timeout.as_millis() as u64),
        };
        let mut state = self.state.lock().await;
        if state.session_grants.contains(&grant_key(&request)) {
            if let Some(option_id) = option_id(&request, PermissionOptionKind::AllowOnce) {
                let record = record(
                    request.clone(),
                    Some(PermissionDecision::AllowSession),
                    PermissionResolutionStatus::Allowed,
                    PermissionResolutionSource::SessionGrant,
                    Some(option_id),
                );
                state.history.push(record.clone());
                return PermissionTicket {
                    request,
                    resolution: TicketResolution::Immediate(Box::new(record)),
                };
            }
        }

        let (response, receiver) = oneshot::channel();
        state.pending.insert(
            request.id.clone(),
            PendingPermission {
                request: request.clone(),
                response,
            },
        );
        PermissionTicket {
            request,
            resolution: TicketResolution::Pending(receiver),
        }
    }

    pub async fn wait(&self, ticket: PermissionTicket) -> PermissionAuditRecord {
        match ticket.resolution {
            TicketResolution::Immediate(record) => *record,
            TicketResolution::Pending(mut receiver) => {
                match tokio::time::timeout(self.timeout, &mut receiver).await {
                    Ok(Ok(record)) => record,
                    Ok(Err(_)) => cancellation_record(ticket.request),
                    Err(_) => match self.expire(&ticket.request.id).await {
                        Some(record) => record,
                        None => receiver
                            .await
                            .unwrap_or_else(|_| cancellation_record(ticket.request)),
                    },
                }
            }
        }
    }

    pub async fn decide(
        &self,
        permission_id: &str,
        session_id: &str,
        run_id: &str,
        decision: PermissionDecision,
    ) -> Result<PermissionAuditRecord, String> {
        let mut state = self.state.lock().await;
        let pending = state
            .pending
            .get(permission_id)
            .ok_or_else(|| format!("Permission request is no longer pending: {permission_id}"))?;
        if pending.request.session_id != session_id || pending.request.run_id != run_id {
            return Err("Permission request provenance does not match".to_owned());
        }

        let (status, selected_option_id) = match decision {
            PermissionDecision::AllowOnce | PermissionDecision::AllowSession => (
                PermissionResolutionStatus::Allowed,
                Some(
                    option_id(&pending.request, PermissionOptionKind::AllowOnce).ok_or_else(
                        || "The adapter did not offer a safely scoped allow-once option".to_owned(),
                    )?,
                ),
            ),
            PermissionDecision::Deny => (
                PermissionResolutionStatus::Denied,
                option_id(&pending.request, PermissionOptionKind::RejectOnce),
            ),
        };

        let pending = state
            .pending
            .remove(permission_id)
            .expect("pending permission disappeared while broker lock was held");
        if decision == PermissionDecision::AllowSession {
            state.session_grants.insert(grant_key(&pending.request));
        }
        let record = record(
            pending.request,
            Some(decision),
            status,
            PermissionResolutionSource::User,
            selected_option_id,
        );
        state.history.push(record.clone());
        let _ = pending.response.send(record.clone());
        Ok(record)
    }

    pub async fn pending(&self, session_id: Option<&str>) -> Vec<PermissionRequest> {
        let state = self.state.lock().await;
        let mut requests: Vec<_> = state
            .pending
            .values()
            .filter(|pending| {
                session_id.is_none_or(|session_id| pending.request.session_id == session_id)
            })
            .map(|pending| pending.request.clone())
            .collect();
        requests.sort_by_key(|request| request.created_at_ms);
        requests
    }

    pub async fn history(&self, session_id: Option<&str>) -> Vec<PermissionAuditRecord> {
        self.state
            .lock()
            .await
            .history
            .iter()
            .filter(|record| {
                session_id.is_none_or(|session_id| record.request.session_id == session_id)
            })
            .cloned()
            .collect()
    }

    pub async fn cancel_run(&self, session_id: &str, run_id: &str) {
        self.cancel_matching(|request| {
            request.session_id == session_id && request.run_id == run_id
        })
        .await;
    }

    pub async fn cancel_session(&self, session_id: &str) {
        self.cancel_matching(|request| request.session_id == session_id)
            .await;
        self.state
            .lock()
            .await
            .session_grants
            .retain(|grant| grant.session_id != session_id);
    }

    async fn expire(&self, permission_id: &str) -> Option<PermissionAuditRecord> {
        let mut state = self.state.lock().await;
        let pending = state.pending.remove(permission_id)?;
        let record = record(
            pending.request,
            None,
            PermissionResolutionStatus::TimedOut,
            PermissionResolutionSource::Timeout,
            None,
        );
        state.history.push(record.clone());
        let _ = pending.response.send(record.clone());
        Some(record)
    }

    async fn cancel_matching(&self, matches: impl Fn(&PermissionRequest) -> bool) {
        let mut state = self.state.lock().await;
        let ids: Vec<_> = state
            .pending
            .iter()
            .filter(|(_, pending)| matches(&pending.request))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            let pending = state
                .pending
                .remove(&id)
                .expect("pending permission disappeared while broker lock was held");
            let record = cancellation_record(pending.request);
            state.history.push(record.clone());
            let _ = pending.response.send(record);
        }
    }
}

fn option_id(request: &PermissionRequest, kind: PermissionOptionKind) -> Option<String> {
    request
        .options
        .iter()
        .find(|option| option.kind == kind)
        .map(|option| option.id.clone())
}

fn grant_key(request: &PermissionRequest) -> GrantKey {
    GrantKey {
        session_id: request.session_id.clone(),
        category: request.category.clone(),
        title: request.title.clone(),
        target: request.target.clone(),
    }
}

fn record(
    request: PermissionRequest,
    decision: Option<PermissionDecision>,
    status: PermissionResolutionStatus,
    source: PermissionResolutionSource,
    selected_option_id: Option<String>,
) -> PermissionAuditRecord {
    PermissionAuditRecord {
        request,
        decision,
        status,
        source,
        selected_option_id,
        resolved_at_ms: now_ms(),
    }
}

fn cancellation_record(request: PermissionRequest) -> PermissionAuditRecord {
    record(
        request,
        None,
        PermissionResolutionStatus::Cancelled,
        PermissionResolutionSource::Cancellation,
        None,
    )
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    fn intent(session: &str, run: &str, target: &str) -> PermissionIntent {
        PermissionIntent {
            agent_id: "fake".to_owned(),
            session_id: session.to_owned(),
            run_id: run.to_owned(),
            request_id: format!("ipc-{run}"),
            tool_call_id: format!("tool-{run}"),
            category: PermissionCategory::FilesystemWrite,
            title: "Write file".to_owned(),
            target: Some(target.to_owned()),
        }
    }

    fn options() -> Vec<PermissionOption> {
        vec![
            PermissionOption {
                id: "allow".to_owned(),
                name: "Allow once".to_owned(),
                kind: PermissionOptionKind::AllowOnce,
            },
            PermissionOption {
                id: "reject".to_owned(),
                name: "Reject".to_owned(),
                kind: PermissionOptionKind::RejectOnce,
            },
        ]
    }

    #[tokio::test]
    async fn allow_once_resolves_only_one_pending_request() {
        let broker = PermissionBroker::new(Duration::from_secs(1));
        let ticket = broker
            .register(intent("session-1", "run-1", "a.txt"), options())
            .await;
        let request = ticket.request.clone();
        let resolved = broker
            .decide(
                &request.id,
                &request.session_id,
                &request.run_id,
                PermissionDecision::AllowOnce,
            )
            .await
            .unwrap();
        assert_eq!(resolved.status, PermissionResolutionStatus::Allowed);
        assert_eq!(broker.wait(ticket).await, resolved);
        assert!(broker
            .decide(
                &request.id,
                &request.session_id,
                &request.run_id,
                PermissionDecision::Deny,
            )
            .await
            .is_err());
    }

    #[tokio::test]
    async fn session_grant_is_exact_and_does_not_leak_to_another_session() {
        let broker = PermissionBroker::new(Duration::from_secs(1));
        let first = broker
            .register(intent("session-1", "run-1", "a.txt"), options())
            .await;
        broker
            .decide(
                &first.request.id,
                "session-1",
                "run-1",
                PermissionDecision::AllowSession,
            )
            .await
            .unwrap();
        assert_eq!(
            broker.wait(first).await.source,
            PermissionResolutionSource::User
        );

        let granted = broker
            .register(intent("session-1", "run-2", "a.txt"), options())
            .await;
        assert_eq!(
            broker.wait(granted).await.source,
            PermissionResolutionSource::SessionGrant
        );

        let different_target = broker
            .register(intent("session-1", "run-3", "b.txt"), options())
            .await;
        let other_session = broker
            .register(intent("session-2", "run-4", "a.txt"), options())
            .await;
        assert_eq!(broker.pending(None).await.len(), 2);
        broker.cancel_session("session-1").await;
        broker.cancel_session("session-2").await;
        assert_eq!(
            broker.wait(different_target).await.status,
            PermissionResolutionStatus::Cancelled
        );
        assert_eq!(
            broker.wait(other_session).await.status,
            PermissionResolutionStatus::Cancelled
        );
    }

    #[tokio::test]
    async fn concurrent_requests_timeout_or_cancel_without_overwriting() {
        let broker = Arc::new(PermissionBroker::new(Duration::from_millis(20)));
        let first = broker
            .register(intent("session-1", "run-1", "a.txt"), options())
            .await;
        let second = broker
            .register(intent("session-1", "run-1", "b.txt"), options())
            .await;
        assert_ne!(first.request.id, second.request.id);
        assert_eq!(broker.pending(Some("session-1")).await.len(), 2);
        broker
            .decide(
                &first.request.id,
                "session-1",
                "run-1",
                PermissionDecision::Deny,
            )
            .await
            .unwrap();
        assert_eq!(
            broker.wait(first).await.status,
            PermissionResolutionStatus::Denied
        );
        assert_eq!(
            broker.wait(second).await.status,
            PermissionResolutionStatus::TimedOut
        );
        assert_eq!(broker.history(Some("session-1")).await.len(), 2);
    }
}
