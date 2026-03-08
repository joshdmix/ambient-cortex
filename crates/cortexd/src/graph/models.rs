use anyhow::Result;
use chrono::{DateTime, Utc};
use cortex_common::events::{CortexEvent, EventSource, EventType};
use cortex_common::models::{FileNode, Insight, InsightType};
use rusqlite::Row;

pub fn event_from_row(row: &Row) -> Result<CortexEvent, rusqlite::Error> {
    let id: i64 = row.get("id")?;
    let timestamp_str: String = row.get("timestamp")?;
    let event_type_str: String = row.get("event_type")?;
    let source_str: String = row.get("source")?;
    let project: Option<String> = row.get("project")?;
    let file_path: Option<String> = row.get("file_path")?;
    let payload_str: String = row.get("payload")?;
    let session_id: Option<String> = row.get("session_id")?;

    let timestamp: DateTime<Utc> = timestamp_str
        .parse()
        .unwrap_or_else(|_| Utc::now());

    let event_type = deserialize_event_type(&event_type_str);
    let source = deserialize_source(&source_str);
    let payload: serde_json::Value =
        serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);

    Ok(CortexEvent {
        id: Some(id),
        timestamp,
        event_type,
        source,
        project,
        file_path,
        payload,
        session_id,
    })
}

pub fn file_node_from_row(row: &Row) -> Result<FileNode, rusqlite::Error> {
    let id: i64 = row.get("id")?;
    let path: String = row.get("path")?;
    let project: String = row.get("project")?;
    let first_seen_str: String = row.get("first_seen")?;
    let last_touched_str: String = row.get("last_touched")?;
    let touch_count: i64 = row.get("touch_count")?;
    let total_time_s: i64 = row.get("total_time_s")?;
    let tags_str: Option<String> = row.get("tags")?;

    let first_seen: DateTime<Utc> = first_seen_str.parse().unwrap_or_else(|_| Utc::now());
    let last_touched: DateTime<Utc> = last_touched_str.parse().unwrap_or_else(|_| Utc::now());
    let tags: Vec<String> = tags_str
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    Ok(FileNode {
        id: Some(id),
        path,
        project,
        first_seen,
        last_touched,
        touch_count,
        total_time_s,
        tags,
    })
}

pub fn insight_from_row(row: &Row) -> Result<Insight, rusqlite::Error> {
    let id: i64 = row.get("id")?;
    let created_at_str: String = row.get("created_at")?;
    let trigger_event: Option<i64> = row.get("trigger_event")?;
    let insight_type_str: String = row.get("insight_type")?;
    let title: String = row.get("title")?;
    let body: String = row.get("body")?;
    let relevance: f64 = row.get("relevance")?;
    let surfaced: i64 = row.get("surfaced")?;
    let dismissed: i64 = row.get("dismissed")?;
    let file_path: Option<String> = row.get("file_path")?;
    let project: Option<String> = row.get("project")?;

    let created_at: DateTime<Utc> = created_at_str.parse().unwrap_or_else(|_| Utc::now());
    let insight_type = deserialize_insight_type(&insight_type_str);

    Ok(Insight {
        id: Some(id),
        created_at,
        trigger_event,
        insight_type,
        title,
        body,
        relevance,
        surfaced: surfaced != 0,
        dismissed: dismissed != 0,
        file_path,
        project,
    })
}

pub fn serialize_event_type(et: &EventType) -> String {
    match et {
        EventType::FileOpen => "file_open".to_string(),
        EventType::FileSave => "file_save".to_string(),
        EventType::FileDelete => "file_delete".to_string(),
        EventType::CommandRun => "command_run".to_string(),
        EventType::CommandFail => "command_fail".to_string(),
        EventType::GitCommit => "git_commit".to_string(),
        EventType::GitCheckout => "git_checkout".to_string(),
        EventType::GitMerge => "git_merge".to_string(),
        EventType::BuildSuccess => "build_success".to_string(),
        EventType::BuildFail => "build_fail".to_string(),
        EventType::ErrorEncountered => "error_encountered".to_string(),
        EventType::ClaudeSession => "claude_session".to_string(),
    }
}

pub fn deserialize_event_type(s: &str) -> EventType {
    match s {
        "file_open" => EventType::FileOpen,
        "file_save" => EventType::FileSave,
        "file_delete" => EventType::FileDelete,
        "command_run" => EventType::CommandRun,
        "command_fail" => EventType::CommandFail,
        "git_commit" => EventType::GitCommit,
        "git_checkout" => EventType::GitCheckout,
        "git_merge" => EventType::GitMerge,
        "build_success" => EventType::BuildSuccess,
        "build_fail" => EventType::BuildFail,
        "error_encountered" => EventType::ErrorEncountered,
        "claude_session" => EventType::ClaudeSession,
        _ => EventType::FileSave, // fallback
    }
}

pub fn serialize_source(s: &EventSource) -> String {
    match s {
        EventSource::Terminal => "terminal".to_string(),
        EventSource::Filesystem => "filesystem".to_string(),
        EventSource::Git => "git".to_string(),
        EventSource::Editor => "editor".to_string(),
    }
}

pub fn deserialize_source(s: &str) -> EventSource {
    match s {
        "terminal" => EventSource::Terminal,
        "filesystem" => EventSource::Filesystem,
        "git" => EventSource::Git,
        "editor" => EventSource::Editor,
        _ => EventSource::Filesystem,
    }
}

pub fn serialize_insight_type(it: &InsightType) -> String {
    match it {
        InsightType::Warning => "warning".to_string(),
        InsightType::Reminder => "reminder".to_string(),
        InsightType::Suggestion => "suggestion".to_string(),
        InsightType::History => "history".to_string(),
    }
}

pub fn deserialize_insight_type(s: &str) -> InsightType {
    match s {
        "warning" => InsightType::Warning,
        "reminder" => InsightType::Reminder,
        "suggestion" => InsightType::Suggestion,
        "history" => InsightType::History,
        _ => InsightType::Suggestion,
    }
}

pub fn serialize_relation_type(rt: &cortex_common::models::RelationType) -> String {
    match rt {
        cortex_common::models::RelationType::CoEdited => "co_edited".to_string(),
        cortex_common::models::RelationType::Imports => "imports".to_string(),
        cortex_common::models::RelationType::BreaksWhenChanged => "breaks_when_changed".to_string(),
        cortex_common::models::RelationType::TestFor => "test_for".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cortex_common::events::{EventSource, EventType};
    use cortex_common::models::{InsightType, RelationType};

    // ---------- EventType roundtrip ----------

    #[test]
    fn event_type_roundtrip_file_open() {
        let s = serialize_event_type(&EventType::FileOpen);
        assert_eq!(s, "file_open");
        assert!(matches!(deserialize_event_type(&s), EventType::FileOpen));
    }

    #[test]
    fn event_type_roundtrip_file_save() {
        let s = serialize_event_type(&EventType::FileSave);
        assert_eq!(s, "file_save");
        assert!(matches!(deserialize_event_type(&s), EventType::FileSave));
    }

    #[test]
    fn event_type_roundtrip_file_delete() {
        let s = serialize_event_type(&EventType::FileDelete);
        assert_eq!(s, "file_delete");
        assert!(matches!(deserialize_event_type(&s), EventType::FileDelete));
    }

    #[test]
    fn event_type_roundtrip_command_run() {
        let s = serialize_event_type(&EventType::CommandRun);
        assert_eq!(s, "command_run");
        assert!(matches!(deserialize_event_type(&s), EventType::CommandRun));
    }

    #[test]
    fn event_type_roundtrip_command_fail() {
        let s = serialize_event_type(&EventType::CommandFail);
        assert_eq!(s, "command_fail");
        assert!(matches!(deserialize_event_type(&s), EventType::CommandFail));
    }

    #[test]
    fn event_type_roundtrip_git_commit() {
        let s = serialize_event_type(&EventType::GitCommit);
        assert_eq!(s, "git_commit");
        assert!(matches!(deserialize_event_type(&s), EventType::GitCommit));
    }

    #[test]
    fn event_type_roundtrip_git_checkout() {
        let s = serialize_event_type(&EventType::GitCheckout);
        assert_eq!(s, "git_checkout");
        assert!(matches!(deserialize_event_type(&s), EventType::GitCheckout));
    }

    #[test]
    fn event_type_roundtrip_git_merge() {
        let s = serialize_event_type(&EventType::GitMerge);
        assert_eq!(s, "git_merge");
        assert!(matches!(deserialize_event_type(&s), EventType::GitMerge));
    }

    #[test]
    fn event_type_roundtrip_build_success() {
        let s = serialize_event_type(&EventType::BuildSuccess);
        assert_eq!(s, "build_success");
        assert!(matches!(deserialize_event_type(&s), EventType::BuildSuccess));
    }

    #[test]
    fn event_type_roundtrip_build_fail() {
        let s = serialize_event_type(&EventType::BuildFail);
        assert_eq!(s, "build_fail");
        assert!(matches!(deserialize_event_type(&s), EventType::BuildFail));
    }

    #[test]
    fn event_type_roundtrip_error_encountered() {
        let s = serialize_event_type(&EventType::ErrorEncountered);
        assert_eq!(s, "error_encountered");
        assert!(matches!(deserialize_event_type(&s), EventType::ErrorEncountered));
    }

    #[test]
    fn event_type_roundtrip_claude_session() {
        let s = serialize_event_type(&EventType::ClaudeSession);
        assert_eq!(s, "claude_session");
        assert!(matches!(deserialize_event_type(&s), EventType::ClaudeSession));
    }

    #[test]
    fn event_type_unknown_falls_back_to_file_save() {
        assert!(matches!(deserialize_event_type("nonsense"), EventType::FileSave));
        assert!(matches!(deserialize_event_type(""), EventType::FileSave));
        assert!(matches!(deserialize_event_type("FILE_OPEN"), EventType::FileSave));
    }

    // ---------- EventSource roundtrip ----------

    #[test]
    fn source_roundtrip_terminal() {
        let s = serialize_source(&EventSource::Terminal);
        assert_eq!(s, "terminal");
        assert!(matches!(deserialize_source(&s), EventSource::Terminal));
    }

    #[test]
    fn source_roundtrip_filesystem() {
        let s = serialize_source(&EventSource::Filesystem);
        assert_eq!(s, "filesystem");
        assert!(matches!(deserialize_source(&s), EventSource::Filesystem));
    }

    #[test]
    fn source_roundtrip_git() {
        let s = serialize_source(&EventSource::Git);
        assert_eq!(s, "git");
        assert!(matches!(deserialize_source(&s), EventSource::Git));
    }

    #[test]
    fn source_roundtrip_editor() {
        let s = serialize_source(&EventSource::Editor);
        assert_eq!(s, "editor");
        assert!(matches!(deserialize_source(&s), EventSource::Editor));
    }

    #[test]
    fn source_unknown_falls_back_to_filesystem() {
        assert!(matches!(deserialize_source("unknown"), EventSource::Filesystem));
        assert!(matches!(deserialize_source(""), EventSource::Filesystem));
        assert!(matches!(deserialize_source("TERMINAL"), EventSource::Filesystem));
    }

    // ---------- InsightType roundtrip ----------

    #[test]
    fn insight_type_roundtrip_warning() {
        let s = serialize_insight_type(&InsightType::Warning);
        assert_eq!(s, "warning");
        assert!(matches!(deserialize_insight_type(&s), InsightType::Warning));
    }

    #[test]
    fn insight_type_roundtrip_reminder() {
        let s = serialize_insight_type(&InsightType::Reminder);
        assert_eq!(s, "reminder");
        assert!(matches!(deserialize_insight_type(&s), InsightType::Reminder));
    }

    #[test]
    fn insight_type_roundtrip_suggestion() {
        let s = serialize_insight_type(&InsightType::Suggestion);
        assert_eq!(s, "suggestion");
        assert!(matches!(deserialize_insight_type(&s), InsightType::Suggestion));
    }

    #[test]
    fn insight_type_roundtrip_history() {
        let s = serialize_insight_type(&InsightType::History);
        assert_eq!(s, "history");
        assert!(matches!(deserialize_insight_type(&s), InsightType::History));
    }

    #[test]
    fn insight_type_unknown_falls_back_to_suggestion() {
        assert!(matches!(deserialize_insight_type("unknown"), InsightType::Suggestion));
        assert!(matches!(deserialize_insight_type(""), InsightType::Suggestion));
        assert!(matches!(deserialize_insight_type("WARNING"), InsightType::Suggestion));
    }

    // ---------- RelationType serialization ----------

    #[test]
    fn relation_type_co_edited() {
        assert_eq!(serialize_relation_type(&RelationType::CoEdited), "co_edited");
    }

    #[test]
    fn relation_type_imports() {
        assert_eq!(serialize_relation_type(&RelationType::Imports), "imports");
    }

    #[test]
    fn relation_type_breaks_when_changed() {
        assert_eq!(serialize_relation_type(&RelationType::BreaksWhenChanged), "breaks_when_changed");
    }

    #[test]
    fn relation_type_test_for() {
        assert_eq!(serialize_relation_type(&RelationType::TestFor), "test_for");
    }
}
