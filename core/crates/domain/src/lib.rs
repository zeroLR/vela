use serde::{Deserialize, Serialize};

pub const IPC_PROTOCOL_MAJOR: u16 = 1;
pub const IPC_PROTOCOL_MINOR: u16 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolVersion {
    pub major: u16,
    pub minor: u16,
}

impl ProtocolVersion {
    pub const CURRENT: Self = Self {
        major: IPC_PROTOCOL_MAJOR,
        minor: IPC_PROTOCOL_MINOR,
    };

    pub fn is_compatible_with(self, other: Self) -> bool {
        self.major == other.major
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Unavailable,
    Ready,
    Unauthenticated,
    Incompatible,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSource {
    BuiltIn,
    UserDefined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDescriptor {
    pub id: String,
    pub display_name: String,
    pub adapter: String,
    pub source: AgentSource,
    pub status: AgentStatus,
    pub executable_path: Option<String>,
    pub version: Option<String>,
    pub protocol_version: Option<String>,
    pub capabilities: Vec<String>,
    pub auth_methods: Vec<String>,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRegistrySnapshot {
    pub generation: u64,
    pub refreshed_at_ms: u64,
    pub agents: Vec<AgentDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Starting,
    Ready,
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionDescriptor {
    pub id: String,
    pub agent_id: String,
    pub acp_session_id: String,
    pub process_id: u32,
    pub cwd: String,
    pub state: SessionState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanEntry {
    pub content: String,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermissionCategory {
    #[serde(rename = "filesystem.read")]
    FilesystemRead,
    #[serde(rename = "filesystem.write")]
    FilesystemWrite,
    #[serde(rename = "shell.execute")]
    ShellExecute,
    #[serde(rename = "network.open_url")]
    NetworkOpenUrl,
    #[serde(rename = "mcp.invoke")]
    McpInvoke,
    #[serde(rename = "other")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionOptionKind {
    AllowOnce,
    AllowAlways,
    RejectOnce,
    RejectAlways,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionOption {
    pub id: String,
    pub name: String,
    pub kind: PermissionOptionKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    pub agent_id: String,
    pub session_id: String,
    pub run_id: String,
    pub request_id: String,
    pub tool_call_id: String,
    pub category: PermissionCategory,
    pub title: String,
    pub target: Option<String>,
    pub options: Vec<PermissionOption>,
    pub created_at_ms: u64,
    pub expires_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    AllowOnce,
    AllowSession,
    Deny,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionResolutionStatus {
    Allowed,
    Denied,
    TimedOut,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionResolutionSource {
    User,
    SessionGrant,
    Timeout,
    Cancellation,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionAuditRecord {
    pub request: PermissionRequest,
    pub decision: Option<PermissionDecision>,
    pub status: PermissionResolutionStatus,
    pub source: PermissionResolutionSource,
    pub selected_option_id: Option<String>,
    pub resolved_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEventPayload {
    TextDelta {
        text: String,
    },
    PlanUpdated {
        entries: Vec<PlanEntry>,
    },
    ToolStarted {
        tool_call_id: String,
        title: String,
    },
    ToolFinished {
        tool_call_id: String,
        status: String,
    },
    PermissionRequested {
        request: PermissionRequest,
    },
    PermissionResolved {
        record: PermissionAuditRecord,
    },
    UsageUpdated {
        used: u64,
        size: u64,
    },
    Completed {
        stop_reason: String,
    },
    Cancelled,
    Failed {
        code: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentEvent {
    pub session_id: String,
    pub run_id: String,
    pub request_id: String,
    pub sequence: u64,
    pub timestamp_ms: u64,
    #[serde(flatten)]
    pub payload: AgentEventPayload,
}

impl AgentEventPayload {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed { .. } | Self::Cancelled | Self::Failed { .. }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::ProtocolVersion;

    #[test]
    fn compatibility_is_decided_by_major_version() {
        assert!(
            ProtocolVersion::CURRENT.is_compatible_with(ProtocolVersion {
                major: 1,
                minor: 99,
            })
        );
        assert!(
            !ProtocolVersion::CURRENT.is_compatible_with(ProtocolVersion { major: 2, minor: 0 })
        );
    }
}
