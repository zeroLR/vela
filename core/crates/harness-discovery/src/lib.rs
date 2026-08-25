use std::{
    collections::HashSet,
    ffi::OsString,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use acp_runtime::{AcpLaunchSpec, InitializeFailure};
use domain::{AgentDescriptor, AgentRegistrySnapshot, AgentSource, AgentStatus};
use serde::Deserialize;
use tokio::{process::Command, sync::RwLock, task::JoinSet};

#[derive(Debug, Clone)]
pub struct DiscoveryOptions {
    pub path: Option<OsString>,
    pub known_directories: Vec<PathBuf>,
    pub config_path: Option<PathBuf>,
    pub version_timeout: Duration,
    pub initialize_timeout: Duration,
}

impl DiscoveryOptions {
    pub fn from_environment() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            known_directories: known_macos_directories(),
            config_path: std::env::var_os("VELA_HARNESS_CONFIG").map(PathBuf::from),
            version_timeout: Duration::from_secs(2),
            initialize_timeout: Duration::from_secs(5),
        }
    }
}

pub struct DiscoveryService {
    options: DiscoveryOptions,
    generation: AtomicU64,
    snapshot: RwLock<AgentRegistrySnapshot>,
}

impl DiscoveryService {
    pub fn new(options: DiscoveryOptions) -> Self {
        Self {
            options,
            generation: AtomicU64::new(0),
            snapshot: RwLock::new(AgentRegistrySnapshot {
                generation: 0,
                refreshed_at_ms: 0,
                agents: Vec::new(),
            }),
        }
    }

    pub async fn snapshot(&self) -> AgentRegistrySnapshot {
        self.snapshot.read().await.clone()
    }

    pub async fn launch_spec(&self, agent_id: &str) -> Result<AcpLaunchSpec, String> {
        let snapshot = self.snapshot.read().await;
        let agent = snapshot
            .agents
            .iter()
            .find(|agent| agent.id == agent_id)
            .ok_or_else(|| format!("Unknown agent: {agent_id}"))?;
        if agent.status != AgentStatus::Ready {
            return Err(format!("Agent {agent_id} is not ready"));
        }
        let missing: Vec<&str> = ["session.new", "session.prompt", "session.cancel"]
            .into_iter()
            .filter(|capability| !agent.capabilities.iter().any(|item| item == capability))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "Agent {agent_id} lacks required capabilities: {}",
                missing.join(", ")
            ));
        }
        let executable = agent
            .executable_path
            .as_deref()
            .ok_or_else(|| format!("Agent {agent_id} has no executable path"))?;
        let (definitions, _) = load_definitions(self.options.config_path.as_deref());
        let definition = definitions
            .into_iter()
            .find(|definition| definition.id == agent_id)
            .ok_or_else(|| "Agent definition changed; refresh discovery".to_owned())?;
        Ok(AcpLaunchSpec {
            executable: PathBuf::from(executable),
            arguments: definition.launch_arguments,
        })
    }

    pub async fn refresh(&self) -> AgentRegistrySnapshot {
        let (definitions, mut agents) = load_definitions(self.options.config_path.as_deref());
        let mut tasks = JoinSet::new();
        for definition in definitions {
            let options = self.options.clone();
            tasks.spawn(async move { discover(definition, options).await });
        }
        while let Some(result) = tasks.join_next().await {
            match result {
                Ok(agent) => agents.push(agent),
                Err(error) => agents.push(configuration_error(format!(
                    "Discovery task failed: {error}"
                ))),
            }
        }
        agents.sort_by(|left, right| left.id.cmp(&right.id));

        let snapshot = AgentRegistrySnapshot {
            generation: self.generation.fetch_add(1, Ordering::Relaxed) + 1,
            refreshed_at_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or_default(),
            agents,
        };
        *self.snapshot.write().await = snapshot.clone();
        snapshot
    }
}

#[derive(Debug, Clone)]
struct HarnessDefinition {
    id: String,
    display_name: String,
    adapter: String,
    command: String,
    provider_command: Option<String>,
    version_arguments: Vec<String>,
    launch_arguments: Vec<String>,
    source: AgentSource,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct HarnessConfigFile {
    #[serde(default)]
    harnesses: Vec<UserHarnessDefinition>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UserHarnessDefinition {
    id: String,
    display_name: String,
    command: String,
    #[serde(default = "custom_adapter")]
    adapter: String,
    #[serde(default = "version_arguments")]
    version_arguments: Vec<String>,
    #[serde(default)]
    launch_arguments: Vec<String>,
}

fn custom_adapter() -> String {
    "custom-acp".to_owned()
}

fn version_arguments() -> Vec<String> {
    vec!["--version".to_owned()]
}

fn built_in_definitions() -> Vec<HarnessDefinition> {
    vec![
        HarnessDefinition {
            id: "claude".to_owned(),
            display_name: "Claude Agent".to_owned(),
            adapter: "claude-agent-acp".to_owned(),
            command: "claude-agent-acp".to_owned(),
            provider_command: Some("claude".to_owned()),
            version_arguments: version_arguments(),
            launch_arguments: Vec::new(),
            source: AgentSource::BuiltIn,
        },
        HarnessDefinition {
            id: "codex".to_owned(),
            display_name: "Codex".to_owned(),
            adapter: "codex-acp".to_owned(),
            command: "codex-acp".to_owned(),
            provider_command: Some("codex".to_owned()),
            version_arguments: version_arguments(),
            launch_arguments: Vec::new(),
            source: AgentSource::BuiltIn,
        },
    ]
}

fn load_definitions(config_path: Option<&Path>) -> (Vec<HarnessDefinition>, Vec<AgentDescriptor>) {
    let mut definitions = built_in_definitions();
    let mut failures = Vec::new();
    let Some(path) = config_path else {
        return (definitions, failures);
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => {
            failures.push(configuration_error(format!(
                "Could not read {}: {error}",
                path.display()
            )));
            return (definitions, failures);
        }
    };
    let config = match serde_json::from_slice::<HarnessConfigFile>(&bytes) {
        Ok(config) => config,
        Err(error) => {
            failures.push(configuration_error(format!(
                "Invalid harness config {}: {error}",
                path.display()
            )));
            return (definitions, failures);
        }
    };

    let mut ids: HashSet<String> = definitions.iter().map(|item| item.id.clone()).collect();
    for (index, item) in config.harnesses.into_iter().enumerate() {
        if !valid_id(&item.id) || item.command.trim().is_empty() || !ids.insert(item.id.clone()) {
            failures.push(configuration_error(format!(
                "Invalid or duplicate user harness at index {index}: {}",
                item.id
            )));
            continue;
        }
        definitions.push(HarnessDefinition {
            id: item.id,
            display_name: item.display_name,
            adapter: item.adapter,
            command: item.command,
            provider_command: None,
            version_arguments: item.version_arguments,
            launch_arguments: item.launch_arguments,
            source: AgentSource::UserDefined,
        });
    }
    (definitions, failures)
}

async fn discover(definition: HarnessDefinition, options: DiscoveryOptions) -> AgentDescriptor {
    let executable = find_executable(
        &definition.command,
        options.path.as_deref(),
        &options.known_directories,
    );
    let Some(executable) = executable else {
        let provider = definition.provider_command.as_deref().and_then(|command| {
            find_executable(command, options.path.as_deref(), &options.known_directories)
        });
        let diagnostic = match (&definition.source, provider) {
            (AgentSource::BuiltIn, Some(path)) => format!(
                "Provider CLI found at {}, but ACP adapter '{}' is unavailable",
                path.display(),
                definition.command
            ),
            (AgentSource::BuiltIn, None) => {
                format!("ACP adapter '{}' was not found", definition.command)
            }
            (AgentSource::UserDefined, _) => {
                format!(
                    "Configured executable '{}' was not found",
                    definition.command
                )
            }
        };
        return descriptor(
            &definition,
            if definition.source == AgentSource::BuiltIn {
                AgentStatus::Unavailable
            } else {
                AgentStatus::Failed
            },
            DescriptorDetails {
                diagnostic: Some(diagnostic),
                ..DescriptorDetails::default()
            },
        );
    };

    let version = match probe_version(
        &executable,
        &definition.version_arguments,
        options.version_timeout,
    )
    .await
    {
        Ok(version) => version,
        Err(error) => {
            return descriptor(
                &definition,
                AgentStatus::Failed,
                DescriptorDetails {
                    executable: Some(executable.clone()),
                    diagnostic: Some(error),
                    ..DescriptorDetails::default()
                },
            );
        }
    };

    let initialize = acp_runtime::initialize(
        AcpLaunchSpec {
            executable: executable.clone(),
            arguments: definition.launch_arguments.clone(),
        },
        options.initialize_timeout,
    )
    .await;
    match initialize {
        Ok(summary) => descriptor(
            &definition,
            AgentStatus::Ready,
            DescriptorDetails {
                executable: Some(executable),
                version: Some(version),
                protocol_version: Some(summary.protocol_version),
                capabilities: summary.capabilities,
                auth_methods: summary.auth_methods,
                diagnostic: None,
            },
        ),
        Err(error) => {
            let status = classify_initialize_failure(&error);
            descriptor(
                &definition,
                status,
                DescriptorDetails {
                    executable: Some(executable),
                    version: Some(version),
                    diagnostic: Some(error.to_string()),
                    ..DescriptorDetails::default()
                },
            )
        }
    }
}

#[derive(Default)]
struct DescriptorDetails {
    executable: Option<PathBuf>,
    version: Option<String>,
    protocol_version: Option<String>,
    capabilities: Vec<String>,
    auth_methods: Vec<String>,
    diagnostic: Option<String>,
}

fn descriptor(
    definition: &HarnessDefinition,
    status: AgentStatus,
    details: DescriptorDetails,
) -> AgentDescriptor {
    AgentDescriptor {
        id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        adapter: definition.adapter.clone(),
        source: definition.source.clone(),
        status,
        executable_path: details
            .executable
            .map(|path| path.to_string_lossy().into_owned()),
        version: details.version,
        protocol_version: details.protocol_version,
        capabilities: details.capabilities,
        auth_methods: details.auth_methods,
        diagnostic: details.diagnostic,
    }
}

fn configuration_error(message: String) -> AgentDescriptor {
    AgentDescriptor {
        id: "user-config".to_owned(),
        display_name: "User Harness Configuration".to_owned(),
        adapter: "custom-acp".to_owned(),
        source: AgentSource::UserDefined,
        status: AgentStatus::Failed,
        executable_path: None,
        version: None,
        protocol_version: None,
        capabilities: Vec::new(),
        auth_methods: Vec::new(),
        diagnostic: Some(message),
    }
}

async fn probe_version(
    executable: &Path,
    arguments: &[String],
    timeout: Duration,
) -> Result<String, String> {
    let mut command = Command::new(executable);
    command.args(arguments).kill_on_drop(true);
    let output = tokio::time::timeout(timeout, command.output())
        .await
        .map_err(|_| format!("Version probe timed out after {timeout:?}"))?
        .map_err(|error| format!("Version probe failed: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let detail = combined
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no version output");
    let detail: String = detail.chars().take(512).collect();
    if output.status.success() {
        Ok(detail)
    } else {
        Err(format!(
            "Version probe exited with {}: {detail}",
            output.status
        ))
    }
}

fn classify_initialize_failure(error: &InitializeFailure) -> AgentStatus {
    let detail = error.to_string().to_ascii_lowercase();
    if detail.contains("auth") || detail.contains("login") || detail.contains("api key") {
        AgentStatus::Unauthenticated
    } else if detail.contains("protocol") || detail.contains("version") {
        AgentStatus::Incompatible
    } else {
        AgentStatus::Failed
    }
}

fn find_executable(
    command: &str,
    path: Option<&std::ffi::OsStr>,
    known_directories: &[PathBuf],
) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return executable_file(command_path);
    }

    let path_directories = path.into_iter().flat_map(std::env::split_paths);
    path_directories
        .chain(known_directories.iter().cloned())
        .find_map(|directory| executable_file(&directory.join(command)))
}

fn executable_file(path: &Path) -> Option<PathBuf> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        return None;
    }
    std::fs::canonicalize(path)
        .ok()
        .or_else(|| Some(path.to_path_buf()))
}

fn known_macos_directories() -> Vec<PathBuf> {
    let mut directories = vec![
        PathBuf::from("/opt/homebrew/bin"),
        PathBuf::from("/usr/local/bin"),
        PathBuf::from("/usr/bin"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        directories.push(home.join(".local/bin"));
        directories.push(home.join(".cargo/bin"));
    }
    directories
}

fn valid_id(id: &str) -> bool {
    let mut characters = id.chars();
    matches!(characters.next(), Some('a'..='z'))
        && characters.all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

#[cfg(test)]
mod tests {
    use std::{
        ffi::OsString,
        fs,
        os::unix::fs::PermissionsExt,
        time::{Duration, SystemTime, UNIX_EPOCH},
    };

    use super::{find_executable, probe_version, valid_id};

    #[test]
    fn path_order_wins_over_known_directories() {
        let root = std::env::temp_dir().join(format!("vela-path-test-{}", std::process::id()));
        let first = root.join("first");
        let known = root.join("known");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&known).unwrap();
        for path in [first.join("agent"), known.join("agent")] {
            fs::write(&path, "#!/bin/sh\n").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = OsString::from(first.as_os_str());

        assert_eq!(
            find_executable("agent", Some(&path), &[known]),
            fs::canonicalize(first.join("agent")).ok()
        );
        assert_eq!(
            find_executable("agent", None, &[root.join("known")]),
            fs::canonicalize(root.join("known/agent")).ok()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn ids_are_stable_and_file_safe() {
        assert!(valid_id("my-agent-2"));
        assert!(!valid_id("My Agent"));
        assert!(!valid_id("2-agent"));
    }

    #[tokio::test]
    async fn version_probe_has_a_timeout_and_preserves_stderr() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("vela-version-test-{nonce}"));
        fs::create_dir_all(&root).unwrap();

        let slow = root.join("slow");
        fs::write(&slow, "#!/bin/sh\nsleep 1\n").unwrap();
        fs::set_permissions(&slow, fs::Permissions::from_mode(0o755)).unwrap();
        let timeout = probe_version(&slow, &[], Duration::from_millis(20))
            .await
            .unwrap_err();
        assert!(timeout.contains("timed out"));

        let failing = root.join("failing");
        fs::write(&failing, "#!/bin/sh\necho broken-version >&2\nexit 7\n").unwrap();
        fs::set_permissions(&failing, fs::Permissions::from_mode(0o755)).unwrap();
        let failure = probe_version(&failing, &[], Duration::from_secs(1))
            .await
            .unwrap_err();
        assert!(failure.contains("broken-version"));

        fs::remove_dir_all(root).unwrap();
    }
}
