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
