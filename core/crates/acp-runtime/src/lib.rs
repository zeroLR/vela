use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex as StdMutex,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use agent_client_protocol::{
    schema::{
        v1::{
            CancelNotification, ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest,
            RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
            SessionNotification, TextContent,
        },
        ProtocolVersion,
    },
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo, Lines,
};
use domain::{AgentEvent, AgentEventPayload, PlanEntry, SessionDescriptor, SessionState};
use futures::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc, oneshot, RwLock};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcpLaunchSpec {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InitializationSummary {
    pub protocol_version: String,
    pub agent_name: Option<String>,
    pub agent_version: Option<String>,
    pub capabilities: Vec<String>,
    pub auth_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitializeFailure {
    Timeout,
    Runtime(String),
}

impl std::fmt::Display for InitializeFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => formatter.write_str("ACP initialize timed out"),
            Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for InitializeFailure {}

pub async fn initialize(
    spec: AcpLaunchSpec,
    timeout: Duration,
) -> Result<InitializationSummary, InitializeFailure> {
    let agent = AcpAgent::new(AcpAgentConfig::new(spec.executable).args(spec.arguments));
    let initialize = Client.builder().name("vela").connect_with(
        agent,
        |connection: ConnectionTo<Agent>| async move {
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await
        },
    );

    let response = tokio::time::timeout(timeout, initialize)
        .await
        .map_err(|_| InitializeFailure::Timeout)?
        .map_err(|error| InitializeFailure::Runtime(error.to_string()))?;
    let value = serde_json::to_value(response)
        .map_err(|error| InitializeFailure::Runtime(error.to_string()))?;
    Ok(normalize_response(&value))
}

fn normalize_response(value: &Value) -> InitializationSummary {
    let capabilities = value
        .get("agentCapabilities")
        .or_else(|| value.get("agent_capabilities"));
    let mut normalized = vec![
        "prompt.text".to_owned(),
        "session.cancel".to_owned(),
        "session.new".to_owned(),
        "session.prompt".to_owned(),
        "session.update".to_owned(),
    ];

    if truthy(capabilities.and_then(|value| value.get("loadSession"))) {
        normalized.push("session.load".to_owned());
    }
    let prompt = capabilities.and_then(|value| value.get("promptCapabilities"));
    for (field, name) in [
        ("audio", "prompt.audio"),
        ("embeddedContext", "prompt.embedded_context"),
        ("image", "prompt.image"),
    ] {
        if truthy(prompt.and_then(|value| value.get(field))) {
            normalized.push(name.to_owned());
        }
    }
    let session = capabilities.and_then(|value| value.get("sessionCapabilities"));
    for (field, name) in [
        ("additionalDirectories", "session.additional_directories"),
        ("close", "session.close"),
        ("delete", "session.delete"),
        ("fork", "session.fork"),
        ("list", "session.list"),
        ("resume", "session.resume"),
    ] {
        if truthy(session.and_then(|value| value.get(field))) {
            normalized.push(name.to_owned());
        }
    }
    let mcp = capabilities.and_then(|value| value.get("mcpCapabilities"));
    for (field, name) in [("http", "mcp.http"), ("sse", "mcp.sse")] {
        if truthy(mcp.and_then(|value| value.get(field))) {
            normalized.push(name.to_owned());
        }
    }
    normalized.sort();
    normalized.dedup();

    let agent_info = value.get("agentInfo").or_else(|| value.get("agent_info"));
    let auth_methods = value
        .get("authMethods")
        .or_else(|| value.get("auth_methods"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|method| {
            method
                .get("id")
                .or_else(|| method.get("name"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();

    InitializationSummary {
        protocol_version: scalar_string(
            value
                .get("protocolVersion")
                .or_else(|| value.get("protocol_version")),
        )
        .unwrap_or_else(|| "1".to_owned()),
        agent_name: agent_info
            .and_then(|value| value.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        agent_version: agent_info
            .and_then(|value| value.get("version"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        capabilities: normalized,
        auth_methods,
    }
}

fn truthy(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(value)) => *value,
        Some(Value::Object(_)) => true,
        _ => false,
    }
}

fn scalar_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptAccepted {
    pub run_id: String,
    pub acp_request_id: String,
}

pub struct SessionManager {
    sessions: RwLock<HashMap<String, Arc<ManagedSession>>>,
    next_id: AtomicU64,
    initialize_timeout: Duration,
    prompt_timeout: Duration,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(Duration::from_secs(10), Duration::from_secs(120))
    }
}

impl SessionManager {
    pub fn new(initialize_timeout: Duration, prompt_timeout: Duration) -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            initialize_timeout,
            prompt_timeout,
        }
    }

    pub async fn create(
        &self,
        agent_id: String,
        spec: AcpLaunchSpec,
        cwd: PathBuf,
    ) -> Result<SessionDescriptor, String> {
        let sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        let session_id = format!("session-{}-{sequence}", std::process::id());
        let session = ManagedSession::spawn(
            session_id.clone(),
            agent_id,
            spec,
            cwd,
            self.initialize_timeout,
            self.prompt_timeout,
        )
        .await?;
        let descriptor = session.descriptor().await;
        self.sessions.write().await.insert(session_id, session);
        Ok(descriptor)
    }

    pub async fn descriptor(&self, session_id: &str) -> Option<SessionDescriptor> {
        let session = self.sessions.read().await.get(session_id).cloned()?;
        Some(session.descriptor().await)
    }

    pub async fn prompt(
        &self,
        session_id: &str,
        request_id: String,
        text: String,
    ) -> Result<(PromptAccepted, broadcast::Receiver<AgentEvent>), String> {
        let session = self
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("Unknown session: {session_id}"))?;
        let receiver = session.events.subscribe();
        let run_sequence = self.next_id.fetch_add(1, Ordering::Relaxed);
        let run_id = format!("run-{}-{run_sequence}", std::process::id());
        let accepted = session.prompt(run_id, request_id, text).await?;
        Ok((accepted, receiver))
    }

    pub async fn cancel(&self, session_id: &str, run_id: &str) -> Result<bool, String> {
        let session = self
            .sessions
            .read()
            .await
            .get(session_id)
            .cloned()
            .ok_or_else(|| format!("Unknown session: {session_id}"))?;
        session.cancel(run_id.to_owned()).await
    }
}

struct ManagedSession {
    id: String,
    agent_id: String,
    acp_session_id: String,
    process_id: u32,
    cwd: PathBuf,
    state: Arc<RwLock<SessionState>>,
    commands: mpsc::Sender<SessionCommand>,
    events: broadcast::Sender<AgentEvent>,
}

impl ManagedSession {
    async fn spawn(
        id: String,
        agent_id: String,
        spec: AcpLaunchSpec,
        cwd: PathBuf,
        initialize_timeout: Duration,
        prompt_timeout: Duration,
    ) -> Result<Arc<Self>, String> {
        let (commands, command_rx) = mpsc::channel(8);
        let (events, _) = broadcast::channel(128);
        let (ready_tx, ready_rx) = oneshot::channel();
        let state = Arc::new(RwLock::new(SessionState::Starting));
        let active_run = Arc::new(StdMutex::new(None));
        let sequence = Arc::new(AtomicU64::new(0));

        let task_state = Arc::clone(&state);
        let process_state = Arc::clone(&state);
        let task_events = events.clone();
        let task_active = Arc::clone(&active_run);
        let task_sequence = Arc::clone(&sequence);
        let task_id = id.clone();
        let task_cwd = cwd.clone();
        tokio::spawn(async move {
            let context = SessionTaskContext {
                session_id: task_id.clone(),
                events: task_events.clone(),
                active_run: Arc::clone(&task_active),
                sequence: Arc::clone(&task_sequence),
                state: process_state,
                prompt_timeout,
            };
            let result = run_session_process(spec, task_cwd, command_rx, ready_tx, context).await;
            if let Err(message) = result {
                *task_state.write().await = SessionState::Failed;
                emit_active(
                    &task_id,
                    &task_active,
                    &task_sequence,
                    &task_events,
                    AgentEventPayload::Failed {
                        code: "agent_process_failed".to_owned(),
                        message,
                    },
                );
            }
        });

        let ready = tokio::time::timeout(initialize_timeout, ready_rx)
            .await
            .map_err(|_| "ACP session initialization timed out".to_owned())?
            .map_err(|_| "ACP session process ended before initialization".to_owned())??;
        *state.write().await = SessionState::Ready;
        tracing::info!(
            component = "acp_session",
            session_id = id,
            acp_session_id = ready.acp_session_id,
            process_id = ready.process_id,
            agent_id,
            "ACP session ready"
        );
        Ok(Arc::new(Self {
            id,
            agent_id,
            acp_session_id: ready.acp_session_id,
            process_id: ready.process_id,
            cwd,
            state,
            commands,
            events,
        }))
    }

    async fn descriptor(&self) -> SessionDescriptor {
        SessionDescriptor {
            id: self.id.clone(),
            agent_id: self.agent_id.clone(),
            acp_session_id: self.acp_session_id.clone(),
            process_id: self.process_id,
            cwd: self.cwd.to_string_lossy().into_owned(),
            state: self.state.read().await.clone(),
        }
    }

    async fn prompt(
        &self,
        run_id: String,
        request_id: String,
        text: String,
    ) -> Result<PromptAccepted, String> {
        let (reply, response) = oneshot::channel();
        let diagnostic_request_id = request_id.clone();
        self.commands
            .send(SessionCommand::Prompt {
                run_id,
                request_id,
                text,
                reply,
            })
            .await
            .map_err(|_| "ACP session process is unavailable".to_owned())?;
        let accepted = response
            .await
            .map_err(|_| "ACP session process ended before accepting prompt".to_owned())??;
        *self.state.write().await = SessionState::Running;
        tracing::info!(
            component = "acp_session",
            session_id = self.id,
            run_id = accepted.run_id,
            request_id = diagnostic_request_id,
            acp_request_id = accepted.acp_request_id,
            process_id = self.process_id,
            "ACP prompt accepted"
        );
        Ok(accepted)
    }

    async fn cancel(&self, run_id: String) -> Result<bool, String> {
        let (reply, response) = oneshot::channel();
        let diagnostic_run_id = run_id.clone();
        self.commands
            .send(SessionCommand::Cancel { run_id, reply })
            .await
            .map_err(|_| "ACP session process is unavailable".to_owned())?;
        let result = response
            .await
            .map_err(|_| "ACP session process ended during cancellation".to_owned())?;
        tracing::info!(
            component = "acp_session",
            session_id = self.id,
            run_id = diagnostic_run_id,
            cancel_requested = result.as_ref().copied().unwrap_or(false),
            "ACP cancellation requested"
        );
        result
    }
}

impl Drop for ManagedSession {
    fn drop(&mut self) {
        let _ = self.commands.try_send(SessionCommand::Shutdown);
    }
}

struct ReadySession {
    acp_session_id: String,
    process_id: u32,
}

struct ActiveRun {
    run_id: String,
    request_id: String,
}

enum SessionCommand {
    Prompt {
        run_id: String,
        request_id: String,
        text: String,
        reply: oneshot::Sender<Result<PromptAccepted, String>>,
    },
    Cancel {
        run_id: String,
        reply: oneshot::Sender<Result<bool, String>>,
    },
    Shutdown,
}

#[derive(Clone)]
struct SessionTaskContext {
    session_id: String,
    events: broadcast::Sender<AgentEvent>,
    active_run: Arc<StdMutex<Option<ActiveRun>>>,
    sequence: Arc<AtomicU64>,
    state: Arc<RwLock<SessionState>>,
    prompt_timeout: Duration,
}

async fn run_session_process(
    spec: AcpLaunchSpec,
    cwd: PathBuf,
    command_rx: mpsc::Receiver<SessionCommand>,
    ready_tx: oneshot::Sender<Result<ReadySession, String>>,
    context: SessionTaskContext,
) -> Result<(), String> {
    let agent = AcpAgent::new(AcpAgentConfig::new(spec.executable).args(spec.arguments));
    let (stdin, stdout, stderr, child) =
        agent.spawn_process().map_err(|error| error.to_string())?;
    let process_id = child.id();
    let mut process = ProcessGuard::new(child);
    let mut stderr_task = tokio::spawn(capture_stderr(stderr));

    let outgoing = futures::sink::unfold(stdin, |mut stdin, line: String| async move {
        stdin.write_all(line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok::<_, std::io::Error>(stdin)
    });
    let incoming = futures::io::BufReader::new(stdout).lines();
    let protocol = run_protocol(
        Lines::new(outgoing, incoming),
        cwd,
        command_rx,
        ready_tx,
        process_id,
        context,
    );
    tokio::pin!(protocol);

    let result = tokio::select! {
        result = &mut protocol => result,
        status = process.wait() => {
            let status = status.map_err(|error| format!("Failed to wait for ACP process: {error}"))?;
            process.terminate();
            let stderr = tokio::time::timeout(Duration::from_millis(250), &mut stderr_task)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            Err(if stderr.is_empty() {
                format!("ACP process exited with {status}")
            } else {
                format!("ACP process exited with {status}: {stderr}")
            })
        }
    };
    process.terminate();
    stderr_task.abort();
    result
}

async fn run_protocol<T>(
    transport: T,
    cwd: PathBuf,
    mut commands: mpsc::Receiver<SessionCommand>,
    ready_tx: oneshot::Sender<Result<ReadySession, String>>,
    process_id: u32,
    context: SessionTaskContext,
) -> Result<(), String>
where
    T: agent_client_protocol::ConnectTo<Client>,
{
    let notification_active = Arc::clone(&context.active_run);
    let notification_sequence = Arc::clone(&context.sequence);
    let notification_events = context.events.clone();
    let permission_active = Arc::clone(&context.active_run);
    let permission_sequence = Arc::clone(&context.sequence);
    let permission_events = context.events.clone();
    let notification_session_id = context.session_id.clone();
    let permission_session_id = context.session_id.clone();
    Client
        .builder()
        .name("vela")
        .on_receive_notification(
            async move |notification: SessionNotification, _cx| {
                if let Ok(value) = serde_json::to_value(notification.update) {
                    if let Some(payload) = normalize_session_update(&value) {
                        emit_active(
                            &notification_session_id,
                            &notification_active,
                            &notification_sequence,
                            &notification_events,
                            payload,
                        );
                    }
                }
                Ok(())
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, _cx| {
                let value = serde_json::to_value(&request).unwrap_or(Value::Null);
                emit_active(
                    &permission_session_id,
                    &permission_active,
                    &permission_sequence,
                    &permission_events,
                    permission_payload(&value),
                );
                responder.respond(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_with(transport, |connection: ConnectionTo<Agent>| async move {
            let SessionTaskContext {
                session_id,
                events,
                active_run,
                sequence,
                state,
                prompt_timeout,
            } = context;
            connection
                .send_request(InitializeRequest::new(ProtocolVersion::V1))
                .block_task()
                .await?;
            let response = connection
                .send_request(NewSessionRequest::new(&cwd))
                .block_task()
                .await?;
            let acp_session_id = response.session_id.to_string();
            let _ = ready_tx.send(Ok(ReadySession {
                acp_session_id: acp_session_id.clone(),
                process_id,
            }));

            while let Some(command) = commands.recv().await {
                match command {
                    SessionCommand::Prompt { run_id, request_id, text, reply } => {
                        let sent = connection.send_request(PromptRequest::new(
                            response.session_id.clone(),
                            vec![ContentBlock::Text(TextContent::new(text))],
                        ));
                        let acp_request_id = sent.id().to_string();
                        *active_run.lock().expect("active run mutex poisoned") = Some(ActiveRun {
                            run_id: run_id.clone(),
                            request_id,
                        });
                        sequence.store(0, Ordering::Relaxed);
                        let (result_tx, mut result_rx) = oneshot::channel();
                        sent.on_receiving_result(async move |result| {
                            let result = result
                                .map(|response| format!("{:?}", response.stop_reason))
                                .map_err(|error| error.to_string());
                            let _ = result_tx.send(result);
                            Ok(())
                        })?;
                        let _ = reply.send(Ok(PromptAccepted { run_id: run_id.clone(), acp_request_id }));

                        let mut timeout = Box::pin(tokio::time::sleep(prompt_timeout));
                        loop {
                            tokio::select! {
                                result = &mut result_rx => {
                                    let payload = match result {
                                        Ok(Ok(reason)) if reason == "Cancelled" => AgentEventPayload::Cancelled,
                                        Ok(Ok(reason)) => AgentEventPayload::Completed { stop_reason: reason },
                                        Ok(Err(message)) => AgentEventPayload::Failed { code: "prompt_failed".to_owned(), message },
                                        Err(_) => AgentEventPayload::Failed { code: "prompt_response_lost".to_owned(), message: "ACP prompt response channel closed".to_owned() },
                                    };
                                    *state.write().await = if matches!(&payload, AgentEventPayload::Cancelled) {
                                        SessionState::Cancelled
                                    } else if matches!(&payload, AgentEventPayload::Failed { .. }) {
                                        SessionState::Failed
                                    } else {
                                        SessionState::Completed
                                    };
                                    emit_active(&session_id, &active_run, &sequence, &events, payload);
                                    clear_active(&active_run);
                                    break;
                                }
                                _ = &mut timeout => {
                                    connection.send_notification(CancelNotification::new(response.session_id.clone()))?;
                                    *state.write().await = SessionState::Failed;
                                    emit_active(&session_id, &active_run, &sequence, &events, AgentEventPayload::Failed {
                                        code: "prompt_timeout".to_owned(),
                                        message: format!("ACP prompt timed out after {prompt_timeout:?}"),
                                    });
                                    clear_active(&active_run);
                                    break;
                                }
                                command = commands.recv() => match command {
                                    Some(SessionCommand::Cancel { run_id: target, reply }) => {
                                        let matches = target == run_id;
                                        if matches {
                                            let result = connection
                                                .send_notification(CancelNotification::new(response.session_id.clone()))
                                                .map(|_| true)
                                                .map_err(|error| error.to_string());
                                            let _ = reply.send(result);
                                        } else {
                                            let _ = reply.send(Ok(false));
                                        }
                                    }
                                    Some(SessionCommand::Prompt { reply, .. }) => {
                                        let _ = reply.send(Err("A prompt is already running".to_owned()));
                                    }
                                    Some(SessionCommand::Shutdown) | None => {
                                        let _ = connection.send_notification(CancelNotification::new(response.session_id.clone()));
                                        return Ok(());
                                    }
                                }
                            }
                        }
                    }
                    SessionCommand::Cancel { reply, .. } => { let _ = reply.send(Ok(false)); }
                    SessionCommand::Shutdown => break,
                }
            }
            Ok(())
        })
        .await
        .map_err(|error| error.to_string())
}

struct ProcessGuard {
    child: async_process::Child,
    process_id: u32,
}

impl ProcessGuard {
    fn new(child: async_process::Child) -> Self {
        let process_id = child.id();
        Self { child, process_id }
    }

    async fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
        self.child.status().await
    }

    fn terminate(&mut self) {
        if let Some(pid) = rustix::process::Pid::from_raw(self.process_id.cast_signed()) {
            let _ = rustix::process::kill_process_group(pid, rustix::process::Signal::KILL);
        }
        let _ = self.child.kill();
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        self.terminate();
    }
}

async fn capture_stderr(mut stderr: async_process::ChildStderr) -> String {
    let mut output = Vec::new();
    let mut buffer = [0; 4096];
    while let Ok(read) = stderr.read(&mut buffer).await {
        if read == 0 {
            break;
        }
        output.extend_from_slice(&buffer[..read]);
        if output.len() > 64 * 1024 {
            output.drain(..output.len() - 64 * 1024);
        }
    }
    String::from_utf8_lossy(&output).trim().to_owned()
}

fn clear_active(active: &StdMutex<Option<ActiveRun>>) {
    active.lock().expect("active run mutex poisoned").take();
}

fn emit_active(
    session_id: &str,
    active: &StdMutex<Option<ActiveRun>>,
    sequence: &AtomicU64,
    events: &broadcast::Sender<AgentEvent>,
    payload: AgentEventPayload,
) {
    let active = active.lock().expect("active run mutex poisoned");
    let Some(run) = active.as_ref() else {
        return;
    };
    let event = AgentEvent {
        session_id: session_id.to_owned(),
        run_id: run.run_id.clone(),
        request_id: run.request_id.clone(),
        sequence: sequence.fetch_add(1, Ordering::Relaxed) + 1,
        timestamp_ms: now_ms(),
        payload,
    };
    let _ = events.send(event);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn normalize_session_update(value: &Value) -> Option<AgentEventPayload> {
    match value.get("sessionUpdate")?.as_str()? {
        "agent_message_chunk" => {
            value
                .pointer("/content/text")
                .and_then(Value::as_str)
                .map(|text| AgentEventPayload::TextDelta {
                    text: text.to_owned(),
                })
        }
        "plan" => Some(AgentEventPayload::PlanUpdated {
            entries: value
                .get("entries")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|entry| PlanEntry {
                    content: string_field(entry, "content"),
                    status: string_field(entry, "status"),
                    priority: string_field(entry, "priority"),
                })
                .collect(),
        }),
        "tool_call" => Some(AgentEventPayload::ToolStarted {
            tool_call_id: string_field(value, "toolCallId"),
            title: string_field(value, "title"),
        }),
        "tool_call_update" => Some(AgentEventPayload::ToolFinished {
            tool_call_id: string_field(value, "toolCallId"),
            status: string_field(value, "status"),
        }),
        "usage_update" => Some(AgentEventPayload::UsageUpdated {
            used: value
                .get("used")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
            size: value
                .get("size")
                .and_then(Value::as_u64)
                .unwrap_or_default(),
        }),
        _ => None,
    }
}

fn permission_payload(value: &Value) -> AgentEventPayload {
    AgentEventPayload::PermissionRequested {
        tool_call_id: value
            .pointer("/toolCall/toolCallId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        title: value
            .pointer("/toolCall/title")
            .and_then(Value::as_str)
            .map(str::to_owned),
        options: value
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|option| {
                option
                    .get("name")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect(),
    }
}

fn string_field(value: &Value, field: &str) -> String {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::normalize_response;

    #[test]
    fn normalizes_wire_capabilities_without_exporting_acp_types() {
        let response = json!({
            "protocolVersion": 1,
            "agentCapabilities": {
                "loadSession": true,
                "promptCapabilities": { "image": true, "audio": false },
                "sessionCapabilities": { "list": {}, "resume": {} }
            },
            "authMethods": [{ "id": "chatgpt" }],
            "agentInfo": { "name": "Fake", "version": "1.2.3" }
        });

        let summary = normalize_response(&response);
        assert_eq!(summary.protocol_version, "1");
        assert_eq!(summary.agent_name.as_deref(), Some("Fake"));
        assert!(summary.capabilities.contains(&"prompt.image".to_owned()));
        assert!(summary.capabilities.contains(&"session.list".to_owned()));
        assert_eq!(summary.auth_methods, ["chatgpt"]);
    }
}
