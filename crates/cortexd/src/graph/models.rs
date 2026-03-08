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
