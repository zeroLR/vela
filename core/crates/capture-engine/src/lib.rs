use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use domain::{
    CaptureIntent, CaptureMetrics, CaptureRecord, CaptureSource, CaptureStatus, WorkspaceProvenance,
};
use workspace_engine::{Workspace, WorkspaceError};

static NEXT_CAPTURE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureError(pub String);

impl std::fmt::Display for CaptureError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for CaptureError {}

impl From<std::io::Error> for CaptureError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for CaptureError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<WorkspaceError> for CaptureError {
    fn from(error: WorkspaceError) -> Self {
        Self(error.to_string())
    }
}

pub struct CaptureEngine;

impl CaptureEngine {
    pub fn create(
        workspace: &Workspace,
        source: CaptureSource,
        raw_text: &str,
        selected_intent: Option<CaptureIntent>,
        started_at_ms: u64,
        correlation_id: Option<&str>,
    ) -> Result<CaptureRecord, CaptureError> {
        let normalized_text = normalize(raw_text);
        if normalized_text.is_empty() {
            return Err(CaptureError("Capture text cannot be empty".to_owned()));
        }
        let suggested_intent = classify(&normalized_text);
        let intent = selected_intent.unwrap_or_else(|| suggested_intent.clone());
        let completed_at_ms = now_ms();
        let mut record = CaptureRecord {
            id: new_capture_id(completed_at_ms),
            source,
            status: CaptureStatus::Completed,
            raw_text: raw_text.to_owned(),
            normalized_text,
            suggested_intent,
            intent,
            title: title(raw_text),
            routed_path: None,
            started_at_ms: started_at_ms.min(completed_at_ms),
            completed_at_ms,
            correction_count: 0,
        };
        record.routed_path = Some(route(workspace, &record, correlation_id)?);
        write_record(workspace, &record, correlation_id)?;
        workspace.record_event(
            "capture.created",
            Some(&record_path(&record.id)),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
        Ok(record)
    }

    pub fn abandon(
        workspace: &Workspace,
        source: CaptureSource,
        raw_text: &str,
        started_at_ms: u64,
        correlation_id: Option<&str>,
    ) -> Result<CaptureRecord, CaptureError> {
        let completed_at_ms = now_ms();
        let record = CaptureRecord {
            id: new_capture_id(completed_at_ms),
            source,
            status: CaptureStatus::Abandoned,
            raw_text: raw_text.to_owned(),
            normalized_text: normalize(raw_text),
            suggested_intent: CaptureIntent::Unknown,
            intent: CaptureIntent::Unknown,
            title: title(raw_text),
            routed_path: None,
            started_at_ms: started_at_ms.min(completed_at_ms),
            completed_at_ms,
            correction_count: 0,
        };
        write_record(workspace, &record, correlation_id)?;
        workspace.record_event(
            "capture.abandoned",
            Some(&record_path(&record.id)),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
        Ok(record)
    }

    pub fn correct(
        workspace: &Workspace,
        capture_id: &str,
        intent: CaptureIntent,
        correlation_id: Option<&str>,
    ) -> Result<CaptureRecord, CaptureError> {
        let mut record = Self::get(workspace, capture_id)?;
        if record.status != CaptureStatus::Completed {
            return Err(CaptureError(
                "An abandoned capture cannot be routed".to_owned(),
            ));
        }
        if record.intent == intent {
            return Ok(record);
        }
        unroute(workspace, &record, correlation_id)?;
        record.intent = intent;
        record.correction_count += 1;
        record.routed_path = Some(route(workspace, &record, correlation_id)?);
        write_record(workspace, &record, correlation_id)?;
        workspace.record_event(
            "capture.corrected",
            Some(&record_path(&record.id)),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
        Ok(record)
    }

    pub fn get(workspace: &Workspace, capture_id: &str) -> Result<CaptureRecord, CaptureError> {
        validate_capture_id(capture_id)?;
        let path = workspace.root().join(record_path(capture_id));
        let bytes = fs::read(&path).map_err(|error| {
            CaptureError(format!(
                "Could not read capture {}: {error}",
                path.display()
            ))
        })?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn list(workspace: &Workspace, limit: usize) -> Result<Vec<CaptureRecord>, CaptureError> {
        let mut records = read_records(workspace)?;
        records.sort_by_key(|record| record.completed_at_ms);
        records.reverse();
        records.truncate(limit.clamp(1, 500));
        Ok(records)
    }

    pub fn metrics(workspace: &Workspace, since_ms: u64) -> Result<CaptureMetrics, CaptureError> {
        let records = read_records(workspace)?;
        let completed = records
            .iter()
            .filter(|record| record.status == CaptureStatus::Completed)
            .collect::<Vec<_>>();
        let corrected_captures = completed
            .iter()
            .filter(|record| record.correction_count > 0)
            .count() as u64;
        let mut durations = completed
            .iter()
            .map(|record| record.completed_at_ms.saturating_sub(record.started_at_ms))
            .collect::<Vec<_>>();
        durations.sort_unstable();
        let median_completion_ms = if durations.is_empty() {
            None
        } else {
            Some(durations[durations.len() / 2])
        };
        let completed_captures = completed.len() as u64;
        Ok(CaptureMetrics {
            total_captures: records.len() as u64,
            completed_captures,
            abandoned_captures: records
                .iter()
                .filter(|record| record.status == CaptureStatus::Abandoned)
                .count() as u64,
            captures_since: records
                .iter()
                .filter(|record| record.completed_at_ms >= since_ms)
                .count() as u64,
            corrected_captures,
            correction_rate_basis_points: if completed_captures == 0 {
                0
            } else {
                (corrected_captures * 10_000 / completed_captures) as u32
            },
            median_completion_ms,
        })
    }
}

fn read_records(workspace: &Workspace) -> Result<Vec<CaptureRecord>, CaptureError> {
    let directory = workspace.root().join("captures");
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut records = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.path().extension().is_none_or(|value| value != "json") {
            continue;
        }
        let bytes = fs::read(entry.path())?;
        records.push(serde_json::from_slice(&bytes)?);
    }
    Ok(records)
}

pub fn classify(text: &str) -> CaptureIntent {
    let value = text.trim().to_lowercase();
    if starts_with_any(
        &value,
        &[
            "todo ",
            "todo:",
            "remember to ",
            "need to ",
            "待辦",
            "記得",
            "要做",
        ],
    ) {
        CaptureIntent::Todo
    } else if starts_with_any(&value, &["idea ", "idea:", "maybe ", "想法", "點子"]) {
        CaptureIntent::Idea
    } else if starts_with_any(
        &value,
        &[
            "working on ",
            "blocked ",
            "blocked:",
            "finished ",
            "目前",
            "進度",
            "卡住",
            "完成",
        ],
    ) {
        CaptureIntent::WorkUpdate
    } else {
        CaptureIntent::Note
    }
}

fn route(
    workspace: &Workspace,
    record: &CaptureRecord,
    correlation_id: Option<&str>,
) -> Result<String, CaptureError> {
    let path = match record.intent {
        CaptureIntent::Note | CaptureIntent::Idea => format!("notes/{}.md", record.id),
        CaptureIntent::Todo => format!("tasks/{}.md", record.id),
        CaptureIntent::WorkUpdate => "STATUS.md".to_owned(),
        CaptureIntent::Unknown => "INBOX.md".to_owned(),
    };
    match record.intent {
        CaptureIntent::WorkUpdate | CaptureIntent::Unknown => {
            let heading = if record.intent == CaptureIntent::WorkUpdate {
                "## Captured work updates"
            } else {
                "## Captured items"
            };
            let current = fs::read_to_string(workspace.root().join(&path))?;
            let updated = add_marker(&current, heading, record);
            workspace.write_file(&path, &updated, WorkspaceProvenance::User, correlation_id)?;
        }
        CaptureIntent::Note | CaptureIntent::Idea | CaptureIntent::Todo => {
            workspace.write_file(
                &path,
                &capture_markdown(record),
                WorkspaceProvenance::User,
                correlation_id,
            )?;
        }
    }
    let event_kind = match record.intent {
        CaptureIntent::Todo => "task.created",
        CaptureIntent::WorkUpdate => "state.updated",
        _ => "capture.routed",
    };
    workspace.record_event(
        event_kind,
        Some(&path),
        WorkspaceProvenance::User,
        correlation_id,
    )?;
    Ok(path)
}

fn unroute(
    workspace: &Workspace,
    record: &CaptureRecord,
    correlation_id: Option<&str>,
) -> Result<(), CaptureError> {
    let Some(path) = &record.routed_path else {
        return Ok(());
    };
    if path == "STATUS.md" || path == "INBOX.md" {
        let current = fs::read_to_string(workspace.root().join(path))?;
        workspace.write_file(
            path,
            &remove_marker(&current, &record.id),
            WorkspaceProvenance::User,
            correlation_id,
        )?;
    } else {
        workspace.remove_file(path, WorkspaceProvenance::User, correlation_id)?;
    }
    Ok(())
}

fn write_record(
    workspace: &Workspace,
    record: &CaptureRecord,
    correlation_id: Option<&str>,
) -> Result<(), CaptureError> {
    let mut json = serde_json::to_string_pretty(record)?;
    json.push('\n');
    workspace.write_file(
        &record_path(&record.id),
        &json,
        WorkspaceProvenance::User,
        correlation_id,
    )?;
    Ok(())
}

fn record_path(capture_id: &str) -> String {
    format!("captures/{capture_id}.json")
}

fn capture_markdown(record: &CaptureRecord) -> String {
    format!(
        "# {}\n\n- Capture: `{}`\n- Intent: `{:?}`\n- Source: `{:?}`\n\n{}\n",
        record.title, record.id, record.intent, record.source, record.raw_text
    )
}

fn add_marker(content: &str, heading: &str, record: &CaptureRecord) -> String {
    let mut value = content.trim_end().to_owned();
    if !value.contains(heading) {
        value.push_str("\n\n");
        value.push_str(heading);
    }
    value.push_str(&format!(
        "\n\n<!-- vela:capture:{}:start -->\n- {}\n<!-- vela:capture:{}:end -->",
        record.id, record.normalized_text, record.id
    ));
    value.push('\n');
    value
}

fn remove_marker(content: &str, capture_id: &str) -> String {
    let start = format!("<!-- vela:capture:{capture_id}:start -->");
    let end = format!("<!-- vela:capture:{capture_id}:end -->");
    let Some(start_index) = content.find(&start) else {
        return content.to_owned();
    };
    let Some(relative_end) = content[start_index..].find(&end) else {
        return content.to_owned();
    };
    let mut end_index = start_index + relative_end + end.len();
    if content.as_bytes().get(end_index) == Some(&b'\n') {
        end_index += 1;
    }
    let mut value = content.to_owned();
    value.replace_range(start_index..end_index, "");
    value
}

fn validate_capture_id(value: &str) -> Result<(), CaptureError> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(CaptureError("Invalid capture ID".to_owned()));
    }
    Ok(())
}

fn starts_with_any(value: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| value.starts_with(prefix))
}

fn normalize(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn title(value: &str) -> String {
    let normalized = normalize(value);
    let title = normalized.chars().take(80).collect::<String>();
    if title.is_empty() {
        "Untitled capture".to_owned()
    } else {
        title
    }
}

fn new_capture_id(timestamp_ms: u64) -> String {
    format!(
        "capture-{timestamp_ms}-{}-{}",
        std::process::id(),
        NEXT_CAPTURE_ID.fetch_add(1, Ordering::Relaxed)
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
    use std::{fs, path::PathBuf};

    use domain::{CaptureIntent, CaptureSource, CaptureStatus};
    use workspace_engine::Workspace;

    use super::CaptureEngine;

    fn temporary_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vela-capture-{name}-{}-{}",
            std::process::id(),
            super::now_ms()
        ))
    }

    #[test]
    fn raw_input_suggestion_and_routed_artifact_survive_reopen() {
        let root = temporary_directory("reopen");
        let workspace = Workspace::open(&root).unwrap();
        let record = CaptureEngine::create(
            &workspace,
            CaptureSource::Text,
            "  todo:   ship the vertical slice  ",
            None,
            super::now_ms() - 20,
            Some("capture-request"),
        )
        .unwrap();

        assert_eq!(record.raw_text, "  todo:   ship the vertical slice  ");
        assert_eq!(record.normalized_text, "todo: ship the vertical slice");
        assert_eq!(record.suggested_intent, CaptureIntent::Todo);
        assert!(root.join(record.routed_path.as_ref().unwrap()).is_file());

        let reopened = Workspace::open(&root).unwrap();
        assert_eq!(CaptureEngine::get(&reopened, &record.id).unwrap(), record);
        assert!(reopened
            .events(50)
            .unwrap()
            .iter()
            .any(|event| event.kind == "capture.created"
                && event.correlation_id.as_deref() == Some("capture-request")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn correction_moves_route_without_losing_original_interpretation() {
        let root = temporary_directory("correction");
        let workspace = Workspace::open(&root).unwrap();
        let original = CaptureEngine::create(
            &workspace,
            CaptureSource::Text,
            "Maybe simplify the capture panel",
            None,
            super::now_ms(),
            None,
        )
        .unwrap();
        let original_path = original.routed_path.clone().unwrap();
        assert_eq!(original.suggested_intent, CaptureIntent::Idea);

        let corrected = CaptureEngine::correct(
            &workspace,
            &original.id,
            CaptureIntent::Todo,
            Some("correct-request"),
        )
        .unwrap();
        assert_eq!(corrected.suggested_intent, CaptureIntent::Idea);
        assert_eq!(corrected.intent, CaptureIntent::Todo);
        assert_eq!(corrected.correction_count, 1);
        assert!(!root.join(original_path).exists());
        assert!(root.join(corrected.routed_path.unwrap()).is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn status_markers_are_removed_when_work_update_is_corrected() {
        let root = temporary_directory("status");
        let workspace = Workspace::open(&root).unwrap();
        let record = CaptureEngine::create(
            &workspace,
            CaptureSource::Speech,
            "目前卡在語音權限",
            Some(CaptureIntent::WorkUpdate),
            super::now_ms(),
            None,
        )
        .unwrap();
        assert!(fs::read_to_string(root.join("STATUS.md"))
            .unwrap()
            .contains(&record.id));

        CaptureEngine::correct(&workspace, &record.id, CaptureIntent::Note, None).unwrap();
        assert!(!fs::read_to_string(root.join("STATUS.md"))
            .unwrap()
            .contains(&record.id));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn metrics_include_completion_correction_and_abandonment() {
        let root = temporary_directory("metrics");
        let workspace = Workspace::open(&root).unwrap();
        let first = CaptureEngine::create(
            &workspace,
            CaptureSource::Text,
            "note one",
            None,
            super::now_ms() - 30,
            None,
        )
        .unwrap();
        CaptureEngine::correct(&workspace, &first.id, CaptureIntent::Todo, None).unwrap();
        CaptureEngine::create(
            &workspace,
            CaptureSource::Text,
            "note two",
            None,
            super::now_ms() - 10,
            None,
        )
        .unwrap();
        let abandoned = CaptureEngine::abandon(
            &workspace,
            CaptureSource::Speech,
            "partial transcript",
            super::now_ms() - 5,
            None,
        )
        .unwrap();
        assert_eq!(abandoned.status, CaptureStatus::Abandoned);

        let metrics = CaptureEngine::metrics(&workspace, 0).unwrap();
        assert_eq!(metrics.total_captures, 3);
        assert_eq!(metrics.completed_captures, 2);
        assert_eq!(metrics.abandoned_captures, 1);
        assert_eq!(metrics.corrected_captures, 1);
        assert_eq!(metrics.correction_rate_basis_points, 5_000);
        assert!(metrics.median_completion_ms.is_some());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn empty_captures_and_unsafe_ids_are_rejected() {
        let root = temporary_directory("validation");
        let workspace = Workspace::open(&root).unwrap();
        assert!(CaptureEngine::create(
            &workspace,
            CaptureSource::Text,
            " \n ",
            None,
            super::now_ms(),
            None,
        )
        .is_err());
        assert!(CaptureEngine::get(&workspace, "../STATUS").is_err());
        fs::create_dir_all(root.join("captures")).unwrap();
        fs::write(root.join("captures/corrupt.json"), "not json").unwrap();
        assert!(CaptureEngine::metrics(&workspace, 0).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
