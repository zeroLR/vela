use std::{
    collections::HashMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use domain::{
    WorkspaceContextFile, WorkspaceContextSlice, WorkspaceEvent, WorkspaceProvenance,
    WorkspaceReference, WorkspaceSnapshot,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tokio::{sync::Mutex, task::JoinHandle};

const DATABASE_RELATIVE_PATH: &str = ".vela/index.sqlite3";
const METADATA_RELATIVE_PATH: &str = ".vela/workspace.json";
const REFERENCES_RELATIVE_PATH: &str = "context/REFERENCES.json";
const CONTEXT_BYTE_LIMIT: usize = 32 * 1024;
const STATUS_TEMPLATE: &str = "# Status\n\n## Active focus\n\n- None\n\n## Blockers\n\n- None\n\n## Next actions\n\n- Define the next useful action.\n";
const INBOX_TEMPLATE: &str = "# Inbox\n\nCapture unprocessed notes and requests here.\n";
static NEXT_TEMPORARY_FILE: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceError(pub String);

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for WorkspaceError {}

impl From<std::io::Error> for WorkspaceError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<rusqlite::Error> for WorkspaceError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for WorkspaceError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct WorkspaceMetadata {
    version: u16,
    created_at_ms: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ReferenceManifest {
    references: Vec<WorkspaceReference>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IndexedFile {
    source: String,
    path: String,
    size_bytes: i64,
    modified_at_ns: i64,
}

#[derive(Debug)]
pub struct Workspace {
    root: PathBuf,
    database_path: PathBuf,
    created_at_ms: u64,
}

impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let requested_root = root.as_ref();
        fs::create_dir_all(requested_root)?;
        let root = fs::canonicalize(requested_root)?;
        if !root.is_dir() {
            return Err(WorkspaceError(format!(
                "Workspace root is not a directory: {}",
                root.display()
            )));
        }

        fs::create_dir_all(root.join(".vela"))?;
        for directory in [
            "projects",
            "tasks",
            "notes",
            "context",
            "decisions",
            "evidence",
        ] {
            fs::create_dir_all(root.join(directory))?;
        }
        write_if_missing(&root, "STATUS.md", STATUS_TEMPLATE)?;
        write_if_missing(&root, "INBOX.md", INBOX_TEMPLATE)?;
        write_json_if_missing(
            &root,
            REFERENCES_RELATIVE_PATH,
            &ReferenceManifest::default(),
        )?;

        let metadata_path = root.join(METADATA_RELATIVE_PATH);
        let new_workspace = !metadata_path.exists();
        if new_workspace {
            write_json_atomic(
                &root,
                METADATA_RELATIVE_PATH,
                &WorkspaceMetadata {
                    version: 1,
                    created_at_ms: now_ms(),
                },
            )?;
        }
        let metadata: WorkspaceMetadata = serde_json::from_slice(&fs::read(&metadata_path)?)
            .map_err(|error| {
                WorkspaceError(format!(
                    "Invalid workspace metadata {}: {error}",
                    metadata_path.display()
                ))
            })?;
        if metadata.version != 1 {
            return Err(WorkspaceError(format!(
                "Unsupported workspace metadata version: {}",
                metadata.version
            )));
        }

        let database_path = root.join(DATABASE_RELATIVE_PATH);
        let new_index = !database_path.exists();
        let workspace = Self {
            root,
            database_path,
            created_at_ms: metadata.created_at_ms,
        };
        workspace.initialize_database()?;
        if new_index {
            workspace.rebuild_index_with_event(
                if new_workspace {
                    "workspace.created"
                } else {
                    "workspace.index_rebuilt"
                },
                WorkspaceProvenance::System,
                None,
            )?;
        } else {
            workspace.reconcile(WorkspaceProvenance::ExternalFilesystem, None)?;
        }
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn snapshot(&self) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let connection = self.connection()?;
        let indexed_file_count = connection
            .query_row("SELECT COUNT(*) FROM files", [], |row| row.get::<_, i64>(0))?
            as u64;
        let last_event_id = connection
            .query_row("SELECT MAX(id) FROM workspace_events", [], |row| {
                row.get::<_, Option<i64>>(0)
            })?
            .map(|value| value as u64);
        Ok(WorkspaceSnapshot {
            root: self.root.to_string_lossy().into_owned(),
            created_at_ms: self.created_at_ms,
            status_markdown: fs::read_to_string(self.root.join("STATUS.md"))?,
            inbox_markdown: fs::read_to_string(self.root.join("INBOX.md"))?,
            references: self.references()?,
            indexed_file_count,
            last_event_id,
        })
    }

    pub fn write_file(
        &self,
        relative_path: &str,
        content: &str,
        provenance: WorkspaceProvenance,
        correlation_id: Option<&str>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let relative = validate_relative_path(relative_path)?;
        if relative.starts_with(".vela") || relative == Path::new(REFERENCES_RELATIVE_PATH) {
            return Err(WorkspaceError(format!(
                "Path is reserved by Vela: {relative_path}"
            )));
        }
        let target = safe_write_target(&self.root, &relative)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        write_bytes_atomic(&self.root, &target, content.as_bytes())?;
        self.reconcile(provenance, correlation_id)?;
        self.snapshot()
    }

    pub fn remove_file(
        &self,
        relative_path: &str,
        provenance: WorkspaceProvenance,
        correlation_id: Option<&str>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let relative = validate_relative_path(relative_path)?;
        if relative.starts_with(".vela") || relative == Path::new(REFERENCES_RELATIVE_PATH) {
            return Err(WorkspaceError(format!(
                "Path is reserved by Vela: {relative_path}"
            )));
        }
        let target = safe_write_target(&self.root, &relative)?;
        if target.exists() {
            let metadata = fs::symlink_metadata(&target)?;
            if !metadata.file_type().is_file() {
                return Err(WorkspaceError(format!(
                    "Only regular workspace files can be removed: {relative_path}"
                )));
            }
            fs::remove_file(target)?;
        }
        self.reconcile(provenance, correlation_id)?;
        self.snapshot()
    }

    pub fn record_event(
        &self,
        kind: &str,
        path: Option<&str>,
        provenance: WorkspaceProvenance,
        correlation_id: Option<&str>,
    ) -> Result<WorkspaceEvent, WorkspaceError> {
        insert_event(&self.connection()?, kind, path, provenance, correlation_id)
    }

    pub fn add_reference(
        &self,
        path: impl AsRef<Path>,
        correlation_id: Option<&str>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let canonical = fs::canonicalize(path.as_ref()).map_err(|error| {
            WorkspaceError(format!(
                "Could not resolve reference {}: {error}",
                path.as_ref().display()
            ))
        })?;
        if !canonical.is_dir() {
            return Err(WorkspaceError(format!(
                "Reference is not a directory: {}",
                canonical.display()
            )));
        }
        if canonical == self.root {
            return Err(WorkspaceError(
                "Workspace root cannot reference itself".to_owned(),
            ));
        }

        let mut manifest = self.reference_manifest()?;
        let canonical_string = canonical.to_string_lossy().into_owned();
        if manifest
            .references
            .iter()
            .any(|reference| reference.path == canonical_string)
        {
            return Err(WorkspaceError(format!(
                "Reference already exists: {canonical_string}"
            )));
        }
        let next_id = manifest
            .references
            .iter()
            .filter_map(|reference| reference.id.strip_prefix("reference-"))
            .filter_map(|value| value.parse::<u64>().ok())
            .max()
            .unwrap_or_default()
            + 1;
        let reference = WorkspaceReference {
            id: format!("reference-{next_id}"),
            path: canonical_string,
            added_at_ms: now_ms(),
        };
        manifest.references.push(reference.clone());
        manifest
            .references
            .sort_by(|left, right| left.id.cmp(&right.id));
        write_json_atomic(&self.root, REFERENCES_RELATIVE_PATH, &manifest)?;

        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO reference_index (id, path, added_at_ms) VALUES (?1, ?2, ?3)",
            params![reference.id, reference.path, reference.added_at_ms as i64],
        )?;
        insert_event(
            &connection,
            "reference.added",
            Some(&reference.path),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
        self.reconcile(WorkspaceProvenance::User, correlation_id)?;
        self.snapshot()
    }

    pub fn remove_reference(
        &self,
        reference_id: &str,
        correlation_id: Option<&str>,
    ) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let mut manifest = self.reference_manifest()?;
        let index = manifest
            .references
            .iter()
            .position(|reference| reference.id == reference_id)
            .ok_or_else(|| WorkspaceError(format!("Unknown reference: {reference_id}")))?;
        let removed = manifest.references.remove(index);
        write_json_atomic(&self.root, REFERENCES_RELATIVE_PATH, &manifest)?;

        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM files WHERE source = ?1", [reference_id])?;
        transaction.execute("DELETE FROM reference_index WHERE id = ?1", [reference_id])?;
        insert_event(
            &transaction,
            "reference.removed",
            Some(&removed.path),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
        transaction.commit()?;
        self.reconcile(WorkspaceProvenance::User, correlation_id)?;
        self.snapshot()
    }

    pub fn reconcile(
        &self,
        provenance: WorkspaceProvenance,
        correlation_id: Option<&str>,
    ) -> Result<Vec<WorkspaceEvent>, WorkspaceError> {
        let current = self.scan_files()?;
        let mut connection = self.connection()?;
        let previous = load_indexed_files(&connection)?;
        let transaction = connection.transaction()?;
        let mut remaining = previous;
        let mut events = Vec::new();

        for file in current {
            let key = (file.source.clone(), file.path.clone());
            let changed = remaining.remove(&key).is_none_or(|old| old != file);
            if !changed {
                continue;
            }
            transaction.execute(
                "INSERT INTO files (source, path, size_bytes, modified_at_ns) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(source, path) DO UPDATE SET size_bytes = excluded.size_bytes, modified_at_ns = excluded.modified_at_ns",
                params![file.source, file.path, file.size_bytes, file.modified_at_ns],
            )?;
            events.push(insert_event(
                &transaction,
                "workspace.file_changed",
                Some(&display_index_path(&file.source, &file.path)),
                provenance.clone(),
                correlation_id,
            )?);
        }

        for ((source, path), _) in remaining {
            transaction.execute(
                "DELETE FROM files WHERE source = ?1 AND path = ?2",
                params![source, path],
            )?;
            events.push(insert_event(
                &transaction,
                "workspace.file_changed",
                Some(&display_index_path(&source, &path)),
                provenance.clone(),
                correlation_id,
            )?);
        }
        transaction.commit()?;
        Ok(events)
    }

    pub fn rebuild_index(&self) -> Result<WorkspaceSnapshot, WorkspaceError> {
        self.initialize_database()?;
        self.rebuild_index_with_event(
            "workspace.index_rebuilt",
            WorkspaceProvenance::System,
            None,
        )?;
        self.snapshot()
    }

    pub fn events(&self, limit: u64) -> Result<Vec<WorkspaceEvent>, WorkspaceError> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, timestamp_ms, kind, path, provenance, correlation_id \
             FROM workspace_events ORDER BY id DESC LIMIT ?1",
        )?;
        let mut events = statement
            .query_map([limit.clamp(1, 500) as i64], |row| {
                Ok(WorkspaceEvent {
                    id: row.get::<_, i64>(0)? as u64,
                    timestamp_ms: row.get::<_, i64>(1)? as u64,
                    kind: row.get(2)?,
                    path: row.get(3)?,
                    provenance: parse_provenance(&row.get::<_, String>(4)?),
                    correlation_id: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        events.reverse();
        Ok(events)
    }

    pub fn context_status(&self) -> Result<WorkspaceContextSlice, WorkspaceError> {
        Ok(WorkspaceContextSlice {
            scope: "status".to_owned(),
            files: vec![
                self.read_workspace_context("STATUS.md")?,
                self.read_workspace_context("INBOX.md")?,
            ],
        })
    }

    pub fn context_workspace_path(
        &self,
        relative_path: &str,
    ) -> Result<WorkspaceContextSlice, WorkspaceError> {
        Ok(WorkspaceContextSlice {
            scope: "workspace_path".to_owned(),
            files: vec![self.read_workspace_context(relative_path)?],
        })
    }

    pub fn context_reference_path(
        &self,
        reference_id: &str,
        relative_path: &str,
    ) -> Result<WorkspaceContextSlice, WorkspaceError> {
        let reference = self
            .references()?
            .into_iter()
            .find(|reference| reference.id == reference_id)
            .ok_or_else(|| WorkspaceError(format!("Unknown reference: {reference_id}")))?;
        let root = PathBuf::from(&reference.path);
        let file = read_context_file(
            &root,
            relative_path,
            &format!("reference:{reference_id}/{relative_path}"),
        )?;
        Ok(WorkspaceContextSlice {
            scope: "reference_path".to_owned(),
            files: vec![file],
        })
    }

    fn read_workspace_context(
        &self,
        relative_path: &str,
    ) -> Result<WorkspaceContextFile, WorkspaceError> {
        read_context_file(&self.root, relative_path, relative_path)
    }

    fn references(&self) -> Result<Vec<WorkspaceReference>, WorkspaceError> {
        let mut references = self.reference_manifest()?.references;
        references.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(references)
    }

    fn reference_manifest(&self) -> Result<ReferenceManifest, WorkspaceError> {
        let path = self.root.join(REFERENCES_RELATIVE_PATH);
        serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
            WorkspaceError(format!(
                "Invalid reference manifest {}: {error}",
                path.display()
            ))
        })
    }

    fn initialize_database(&self) -> Result<(), WorkspaceError> {
        let connection = self.connection()?;
        connection.execute_batch(
            "CREATE TABLE IF NOT EXISTS files (\
                 source TEXT NOT NULL,\
                 path TEXT NOT NULL,\
                 size_bytes INTEGER NOT NULL,\
                 modified_at_ns INTEGER NOT NULL,\
                 PRIMARY KEY (source, path)\
             );\
             CREATE TABLE IF NOT EXISTS reference_index (\
                 id TEXT PRIMARY KEY,\
                 path TEXT NOT NULL UNIQUE,\
                 added_at_ms INTEGER NOT NULL\
             );\
             CREATE TABLE IF NOT EXISTS workspace_events (\
                 id INTEGER PRIMARY KEY AUTOINCREMENT,\
                 timestamp_ms INTEGER NOT NULL,\
                 kind TEXT NOT NULL,\
                 path TEXT,\
                 provenance TEXT NOT NULL,\
                 correlation_id TEXT\
             );",
        )?;
        Ok(())
    }

    fn connection(&self) -> Result<Connection, WorkspaceError> {
        let connection = Connection::open(&self.database_path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        Ok(connection)
    }

    fn rebuild_index_with_event(
        &self,
        kind: &str,
        provenance: WorkspaceProvenance,
        correlation_id: Option<&str>,
    ) -> Result<(), WorkspaceError> {
        let files = self.scan_files()?;
        let references = self.references()?;
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM files", [])?;
        transaction.execute("DELETE FROM reference_index", [])?;
        for file in files {
            transaction.execute(
                "INSERT INTO files (source, path, size_bytes, modified_at_ns) VALUES (?1, ?2, ?3, ?4)",
                params![file.source, file.path, file.size_bytes, file.modified_at_ns],
            )?;
        }
        for reference in references {
            transaction.execute(
                "INSERT INTO reference_index (id, path, added_at_ms) VALUES (?1, ?2, ?3)",
                params![reference.id, reference.path, reference.added_at_ms as i64],
            )?;
        }
        insert_event(&transaction, kind, None, provenance, correlation_id)?;
        transaction.commit()?;
        Ok(())
    }

    fn scan_files(&self) -> Result<Vec<IndexedFile>, WorkspaceError> {
        let mut files = Vec::new();
        scan_directory(&self.root, &self.root, "workspace", &mut files)?;
        for reference in self.references()? {
            let root = PathBuf::from(&reference.path);
            if root.is_dir() {
                scan_directory(&root, &root, &reference.id, &mut files)?;
            }
        }
        files.sort_by(|left, right| (&left.source, &left.path).cmp(&(&right.source, &right.path)));
        Ok(files)
    }
}

pub struct WorkspaceService {
    active: tokio::sync::RwLock<Option<Arc<Workspace>>>,
    watcher: Mutex<Option<JoinHandle<()>>>,
    poll_interval: Duration,
}

impl Default for WorkspaceService {
    fn default() -> Self {
        Self::new(Duration::from_millis(500))
    }
}

impl WorkspaceService {
    pub fn new(poll_interval: Duration) -> Self {
        Self {
            active: tokio::sync::RwLock::new(None),
            watcher: Mutex::new(None),
            poll_interval,
        }
    }

    pub async fn open(&self, root: impl AsRef<Path>) -> Result<WorkspaceSnapshot, WorkspaceError> {
        let workspace = Arc::new(Workspace::open(root)?);
        let snapshot = workspace.snapshot()?;
        *self.active.write().await = Some(Arc::clone(&workspace));

        let mut watcher = self.watcher.lock().await;
        if let Some(task) = watcher.take() {
            task.abort();
        }
        let interval = self.poll_interval;
        *watcher = Some(tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            timer.tick().await;
            loop {
                timer.tick().await;
                let target = Arc::clone(&workspace);
                match tokio::task::spawn_blocking(move || {
                    target.reconcile(WorkspaceProvenance::ExternalFilesystem, None)
                })
                .await
                {
                    Ok(Ok(_)) => {}
                    Ok(Err(error)) => tracing::warn!(
                        component = "workspace_watcher",
                        error = %error,
                        "workspace reconcile failed"
                    ),
                    Err(error) => tracing::warn!(
                        component = "workspace_watcher",
                        error = %error,
                        "workspace reconcile task failed"
                    ),
                }
            }
        }));
        Ok(snapshot)
    }

    pub async fn active(&self) -> Result<Arc<Workspace>, WorkspaceError> {
        self.active
            .read()
            .await
            .clone()
            .ok_or_else(|| WorkspaceError("No workspace is open".to_owned()))
    }
}

fn write_if_missing(root: &Path, relative_path: &str, content: &str) -> Result<(), WorkspaceError> {
    let path = root.join(relative_path);
    if !path.exists() {
        write_bytes_atomic(root, &path, content.as_bytes())?;
    }
    Ok(())
}

fn write_json_if_missing<T: Serialize>(
    root: &Path,
    relative_path: &str,
    value: &T,
) -> Result<(), WorkspaceError> {
    if !root.join(relative_path).exists() {
        write_json_atomic(root, relative_path, value)?;
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(
    root: &Path,
    relative_path: &str,
    value: &T,
) -> Result<(), WorkspaceError> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    let target = root.join(validate_relative_path(relative_path)?);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    write_bytes_atomic(root, &target, &bytes)
}

fn write_bytes_atomic(root: &Path, target: &Path, bytes: &[u8]) -> Result<(), WorkspaceError> {
    let temporary = root.join(".vela").join(format!(
        "write-{}-{}-{}.tmp",
        std::process::id(),
        now_ms(),
        NEXT_TEMPORARY_FILE.fetch_add(1, Ordering::Relaxed)
    ));
    fs::write(&temporary, bytes)?;
    fs::rename(&temporary, target)?;
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<PathBuf, WorkspaceError> {
    let path = Path::new(value);
    if value.trim().is_empty() || path.is_absolute() {
        return Err(WorkspaceError(format!(
            "Expected a non-empty relative path: {value}"
        )));
    }
    if !path
        .components()
        .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(WorkspaceError(format!("Unsafe relative path: {value}")));
    }
    Ok(path.to_path_buf())
}

fn safe_write_target(root: &Path, relative: &Path) -> Result<PathBuf, WorkspaceError> {
    let target = root.join(relative);
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(component) = component else {
            return Err(WorkspaceError("Unsafe path component".to_owned()));
        };
        current.push(component);
        if current.exists() && fs::symlink_metadata(&current)?.file_type().is_symlink() {
            return Err(WorkspaceError(format!(
                "Workspace writes cannot traverse symlinks: {}",
                current.display()
            )));
        }
    }
    Ok(target)
}

fn read_context_file(
    root: &Path,
    relative_path: &str,
    display_path: &str,
) -> Result<WorkspaceContextFile, WorkspaceError> {
    let relative = validate_relative_path(relative_path)?;
    let canonical_root = fs::canonicalize(root)?;
    let canonical = fs::canonicalize(canonical_root.join(relative))?;
    if !canonical.starts_with(&canonical_root) || !canonical.is_file() {
        return Err(WorkspaceError(format!(
            "Context path is outside its root or not a file: {relative_path}"
        )));
    }
    let bytes = fs::read(canonical)?;
    let truncated = bytes.len() > CONTEXT_BYTE_LIMIT;
    let content =
        String::from_utf8_lossy(&bytes[..bytes.len().min(CONTEXT_BYTE_LIMIT)]).into_owned();
    Ok(WorkspaceContextFile {
        path: display_path.to_owned(),
        content,
        truncated,
    })
}

fn scan_directory(
    base: &Path,
    directory: &Path,
    source: &str,
    output: &mut Vec<IndexedFile>,
) -> Result<(), WorkspaceError> {
    let mut entries = fs::read_dir(directory)?.collect::<Result<Vec<_>, _>>()?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if metadata.is_dir() {
            if excluded_directory(&name) {
                continue;
            }
            scan_directory(base, &path, source, output)?;
        } else if metadata.is_file() {
            let relative = path
                .strip_prefix(base)
                .map_err(|error| WorkspaceError(error.to_string()))?
                .to_string_lossy()
                .into_owned();
            output.push(IndexedFile {
                source: source.to_owned(),
                path: relative,
                size_bytes: metadata.len() as i64,
                modified_at_ns: metadata
                    .modified()
                    .ok()
                    .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
                    .map(|value| value.as_nanos() as i64)
                    .unwrap_or_default(),
            });
        }
    }
    Ok(())
}

fn excluded_directory(name: &str) -> bool {
    matches!(
        name,
        ".vela" | ".git" | ".build" | "target" | "node_modules" | ".DS_Store"
    )
}

fn load_indexed_files(
    connection: &Connection,
) -> Result<HashMap<(String, String), IndexedFile>, WorkspaceError> {
    let mut statement =
        connection.prepare("SELECT source, path, size_bytes, modified_at_ns FROM files")?;
    let files = statement.query_map([], |row| {
        Ok(IndexedFile {
            source: row.get(0)?,
            path: row.get(1)?,
            size_bytes: row.get(2)?,
            modified_at_ns: row.get(3)?,
        })
    })?;
    let mut indexed = HashMap::new();
    for file in files {
        let file = file?;
        indexed.insert((file.source.clone(), file.path.clone()), file);
    }
    Ok(indexed)
}

fn insert_event(
    connection: &Connection,
    kind: &str,
    path: Option<&str>,
    provenance: WorkspaceProvenance,
    correlation_id: Option<&str>,
) -> Result<WorkspaceEvent, WorkspaceError> {
    let timestamp_ms = now_ms();
    connection.execute(
        "INSERT INTO workspace_events (timestamp_ms, kind, path, provenance, correlation_id) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            timestamp_ms as i64,
            kind,
            path,
            provenance_name(&provenance),
            correlation_id
        ],
    )?;
    Ok(WorkspaceEvent {
        id: connection.last_insert_rowid() as u64,
        timestamp_ms,
        kind: kind.to_owned(),
        path: path.map(str::to_owned),
        provenance,
        correlation_id: correlation_id.map(str::to_owned),
    })
}

fn display_index_path(source: &str, path: &str) -> String {
    if source == "workspace" {
        path.to_owned()
    } else {
        format!("reference:{source}/{path}")
    }
}

fn provenance_name(provenance: &WorkspaceProvenance) -> &'static str {
    match provenance {
        WorkspaceProvenance::User => "user",
        WorkspaceProvenance::Agent => "agent",
        WorkspaceProvenance::Tool => "tool",
        WorkspaceProvenance::Scheduler => "scheduler",
        WorkspaceProvenance::ExternalFilesystem => "external_filesystem",
        WorkspaceProvenance::System => "system",
    }
}

fn parse_provenance(value: &str) -> WorkspaceProvenance {
    match value {
        "user" => WorkspaceProvenance::User,
        "agent" => WorkspaceProvenance::Agent,
        "tool" => WorkspaceProvenance::Tool,
        "scheduler" => WorkspaceProvenance::Scheduler,
        "external_filesystem" => WorkspaceProvenance::ExternalFilesystem,
        _ => WorkspaceProvenance::System,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use std::{fs, path::PathBuf, time::Duration};

    use domain::{WorkspaceProvenance, WorkspaceReference};

    use super::{Workspace, WorkspaceService};

    fn temporary_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "vela-workspace-{name}-{}-{}",
            std::process::id(),
            super::now_ms()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn create_reopen_and_self_write_are_durable_without_event_loops() {
        let root = temporary_directory("reopen");
        let workspace = Workspace::open(&root).unwrap();
        assert!(root.join("STATUS.md").is_file());
        assert!(root.join("evidence").is_dir());
        let initial_events = workspace.events(100).unwrap().len();
        workspace
            .write_file(
                "STATUS.md",
                "# Status\n\n## Active focus\n\n- Ship Phase 05\n",
                WorkspaceProvenance::User,
                Some("write-1"),
            )
            .unwrap();
        let after_write = workspace.events(100).unwrap();
        assert_eq!(after_write.len(), initial_events + 1);
        assert_eq!(
            after_write.last().unwrap().correlation_id.as_deref(),
            Some("write-1")
        );
        assert!(workspace
            .reconcile(WorkspaceProvenance::ExternalFilesystem, None)
            .unwrap()
            .is_empty());

        let reopened = Workspace::open(&root).unwrap();
        assert!(reopened
            .snapshot()
            .unwrap()
            .status_markdown
            .contains("Ship Phase 05"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn deleting_sqlite_rebuilds_from_canonical_files_and_references() {
        let root = temporary_directory("rebuild");
        let referenced = temporary_directory("reference-rebuild");
        fs::write(referenced.join("README.md"), "reference content").unwrap();
        let workspace = Workspace::open(&root).unwrap();
        workspace
            .add_reference(&referenced, Some("reference-1"))
            .unwrap();
        workspace
            .write_file(
                "notes/durable.md",
                "canonical",
                WorkspaceProvenance::User,
                None,
            )
            .unwrap();
        fs::remove_file(workspace.database_path()).unwrap();

        let rebuilt = Workspace::open(&root).unwrap();
        let snapshot = rebuilt.snapshot().unwrap();
        assert!(root.join("notes/durable.md").is_file());
        assert_eq!(snapshot.references.len(), 1);
        assert!(snapshot.indexed_file_count >= 5);
        assert_eq!(
            rebuilt.events(10).unwrap().last().unwrap().kind,
            "workspace.index_rebuilt"
        );
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(referenced).unwrap();
    }

    #[tokio::test]
    async fn polling_watcher_detects_external_mutation_and_recovers() {
        let root = temporary_directory("watcher");
        let service = WorkspaceService::new(Duration::from_millis(20));
        service.open(&root).await.unwrap();
        fs::write(root.join("notes/external.md"), "outside Vela").unwrap();

        let workspace = service.active().await.unwrap();
        for _ in 0..50 {
            if workspace.events(100).unwrap().iter().any(|event| {
                event.path.as_deref() == Some("notes/external.md")
                    && event.provenance == WorkspaceProvenance::ExternalFilesystem
            }) {
                fs::remove_dir_all(root).unwrap();
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("watcher did not index external mutation");
    }

    #[test]
    fn removing_reference_never_deletes_external_content() {
        let root = temporary_directory("reference-remove");
        let referenced = temporary_directory("referenced-content");
        fs::write(referenced.join("keep.md"), "keep me").unwrap();
        let workspace = Workspace::open(&root).unwrap();
        let snapshot = workspace.add_reference(&referenced, None).unwrap();
        let WorkspaceReference { id, .. } = snapshot.references[0].clone();
        let removed = workspace.remove_reference(&id, Some("remove-1")).unwrap();
        assert!(removed.references.is_empty());
        assert!(referenced.join("keep.md").is_file());
        assert_eq!(
            workspace.events(10).unwrap().last().unwrap().kind,
            "workspace.file_changed"
        );
        assert!(workspace
            .events(10)
            .unwrap()
            .iter()
            .any(|event| event.kind == "reference.removed"));
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(referenced).unwrap();
    }

    #[test]
    fn context_reads_are_explicit_bounded_and_symlink_safe() {
        let root = temporary_directory("context");
        let workspace = Workspace::open(&root).unwrap();
        workspace
            .write_file(
                "evidence/large.txt",
                &"x".repeat(40 * 1024),
                WorkspaceProvenance::User,
                None,
            )
            .unwrap();
        let status = workspace.context_status().unwrap();
        assert_eq!(status.files.len(), 2);
        let evidence = workspace
            .context_workspace_path("evidence/large.txt")
            .unwrap();
        assert!(evidence.files[0].truncated);
        assert!(workspace.context_workspace_path("../outside").is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
