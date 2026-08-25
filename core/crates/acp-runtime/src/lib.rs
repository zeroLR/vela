use std::{path::PathBuf, time::Duration};

use agent_client_protocol::{
    schema::{v1::InitializeRequest, ProtocolVersion},
    AcpAgent, AcpAgentConfig, Agent, Client, ConnectionTo,
};
use serde_json::Value;

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
