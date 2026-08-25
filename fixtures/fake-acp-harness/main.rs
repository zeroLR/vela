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
}

impl Scenario {
    fn parse(value: &str) -> Self {
        match value {
            "timeout" => Self::Timeout,
            "invalid" => Self::Invalid,
            "unauthenticated" => Self::Unauthenticated,
            "incompatible" => Self::Incompatible,
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
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        let request: Value = match serde_json::from_str(&line) {
            Ok(request) => request,
            Err(_) => continue,
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        respond(scenario, id);
    }
}

fn option_value(arguments: &[String], option: &str) -> Option<String> {
    arguments
        .windows(2)
        .find(|items| items[0] == option)
        .map(|items| items[1].clone())
}

fn respond(scenario: Scenario, id: Value) {
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
        Scenario::Ready => write_json(json!({
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

fn write_json(value: Value) {
    write_line(&value.to_string());
}

fn write_line(line: &str) {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{line}").expect("write response");
    stdout.flush().expect("flush response");
}
