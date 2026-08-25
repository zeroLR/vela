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
