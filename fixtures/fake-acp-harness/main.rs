use std::{
    fs,
    io::{self, BufRead, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

use serde_json::{json, Value};

#[derive(Debug, Clone, Copy)]
enum Scenario {
    Ready,
    Timeout,
    Invalid,
    Unauthenticated,
    Incompatible,
    Cancel,
    PromptTimeout,
    UnexpectedExit,
    MalformedEvent,
    Permission,
}

impl Scenario {
    fn parse(value: &str) -> Self {
        match value {
            "timeout" => Self::Timeout,
            "invalid" => Self::Invalid,
            "unauthenticated" => Self::Unauthenticated,
            "incompatible" => Self::Incompatible,
            "cancel" => Self::Cancel,
            "prompt-timeout" => Self::PromptTimeout,
            "unexpected-exit" => Self::UnexpectedExit,
            "malformed-event" => Self::MalformedEvent,
            "permission" => Self::Permission,
            _ => Self::Ready,
        }
    }
}

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if arguments.iter().any(|argument| argument == "--version") {
        println!("fake-acp-harness 0.1.0");
        return;
    }

    let scenario = option_value(&arguments, "--scenario")
        .as_deref()
        .map(Scenario::parse)
        .unwrap_or(Scenario::Ready);
    if let Some(path) = option_value(&arguments, "--pid-file").map(PathBuf::from) {
        fs::write(path, std::process::id().to_string()).expect("write pid file");
    }

    let stdin = io::stdin();
    let mut pending_prompt = None;
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        match request.get("method").and_then(Value::as_str) {
            Some("initialize") => respond_initialize(scenario, id),
            Some("session/new") => write_json(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"sessionId": "fake-session-1"}
            })),
            Some("session/prompt") => respond_prompt(scenario, id, &mut pending_prompt),
            Some("session/cancel") if !matches!(scenario, Scenario::PromptTimeout) => {
                if let Some(prompt_id) = pending_prompt.take() {
                    prompt_response(prompt_id, "cancelled");
                }
            }
            _ if id == json!("permission-1") => {
                if let Some(prompt_id) = pending_prompt.take() {
                    prompt_response(prompt_id, "cancelled");
                }
            }
            _ => {}
        }
    }
}

fn option_value(arguments: &[String], option: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|items| items[0] == option)
        .map(|items| items[1].clone())
}

fn respond_initialize(scenario: Scenario, id: Value) {
    match scenario {
        Scenario::Timeout => thread::sleep(Duration::from_secs(30)),
        Scenario::Invalid => write_line("not-json"),
        Scenario::Unauthenticated => write_json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32000, "message": "authentication required"}
        })),
        Scenario::Incompatible => write_json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32602, "message": "incompatible protocol version"}
        })),
        Scenario::Ready
        | Scenario::Cancel
        | Scenario::PromptTimeout
        | Scenario::UnexpectedExit
        | Scenario::MalformedEvent
        | Scenario::Permission => write_json(json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": 1,
                "agentCapabilities": {
                    "loadSession": true,
                    "promptCapabilities": {
                        "image": true,
                        "audio": false,
                        "embeddedContext": true
                    },
                    "mcpCapabilities": {"http": true, "sse": false},
                    "sessionCapabilities": {"list": {}, "resume": {}}
                },
                "authMethods": [],
                "agentInfo": {"name": "Fake ACP Harness", "version": "0.1.0"}
            }
        })),
    }
}

fn respond_prompt(scenario: Scenario, id: Value, pending_prompt: &mut Option<Value>) {
    match scenario {
        Scenario::Ready => {
            session_update(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Hello "}
            }));
            session_update(json!({
                "sessionUpdate": "plan",
                "entries": [{"content": "Inspect workspace", "priority": "high", "status": "in_progress"}]
            }));
            session_update(json!({
                "sessionUpdate": "tool_call",
                "toolCallId": "tool-1",
                "title": "Read files",
                "kind": "read",
                "status": "in_progress"
            }));
            session_update(json!({
                "sessionUpdate": "tool_call_update",
                "toolCallId": "tool-1",
                "status": "completed"
            }));
            session_update(json!({"sessionUpdate": "usage_update", "used": 12, "size": 4096}));
            session_update(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "from fake ACP."}
            }));
            prompt_response(id, "end_turn");
        }
        Scenario::Cancel => {
            session_update(json!({
                "sessionUpdate": "agent_message_chunk",
                "content": {"type": "text", "text": "Working..."}
            }));
            *pending_prompt = Some(id);
        }
        Scenario::PromptTimeout => *pending_prompt = Some(id),
        Scenario::UnexpectedExit => {
            eprintln!("fake harness exited during prompt");
            std::process::exit(17);
        }
        Scenario::MalformedEvent => {
            write_line("not-json");
            std::process::exit(18);
        }
        Scenario::Permission => {
            *pending_prompt = Some(id);
            write_json(json!({
                "jsonrpc": "2.0",
                "id": "permission-1",
                "method": "session/request_permission",
                "params": {
                    "sessionId": "fake-session-1",
                    "toolCall": {"toolCallId": "tool-2", "title": "Write file", "status": "pending"},
                    "options": [
                        {"optionId": "allow", "name": "Allow once", "kind": "allow_once"},
                        {"optionId": "reject", "name": "Reject", "kind": "reject_once"}
                    ]
                }
            }));
        }
        Scenario::Timeout
        | Scenario::Invalid
        | Scenario::Unauthenticated
        | Scenario::Incompatible => {}
    }
}

fn session_update(update: Value) {
    write_json(json!({
        "jsonrpc": "2.0",
        "method": "session/update",
        "params": {"sessionId": "fake-session-1", "update": update}
    }));
}

fn prompt_response(id: Value, stop_reason: &str) {
    write_json(json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"stopReason": stop_reason}
    }));
}

fn write_json(value: Value) {
    write_line(&value.to_string());
}

fn write_line(line: &str) {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").expect("write response");
    stdout.flush().expect("flush response");
}
