use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CortexEvent {
    pub id: Option<i64>,
    pub timestamp: DateTime<Utc>,
    pub event_type: EventType,
    pub source: EventSource,
    pub project: Option<String>,
    pub file_path: Option<String>,
    pub payload: serde_json::Value,
    pub session_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    FileOpen,
    FileSave,
    FileDelete,
    CommandRun,
    CommandFail,
    GitCommit,
    GitCheckout,
    GitMerge,
    BuildSuccess,
    BuildFail,
    ErrorEncountered,
    ClaudeSession,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum EventSource {
    Terminal,
    Filesystem,
    Git,
    Editor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn sample_timestamp() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap()
    }

    fn roundtrip<T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug>(val: &T) -> T {
        let json = serde_json::to_string(val).expect("serialize");
        serde_json::from_str(&json).expect("deserialize")
    }

    // ---- EventType serde roundtrip ----

    #[test]
    fn event_type_file_open() {
        let v = EventType::FileOpen;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"file_open\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::FileOpen));
    }

    #[test]
    fn event_type_file_save() {
        let v = EventType::FileSave;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"file_save\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::FileSave));
    }

    #[test]
    fn event_type_file_delete() {
        let v = EventType::FileDelete;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"file_delete\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::FileDelete));
    }

    #[test]
    fn event_type_command_run() {
        let v = EventType::CommandRun;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"command_run\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::CommandRun));
    }

    #[test]
    fn event_type_command_fail() {
        let v = EventType::CommandFail;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"command_fail\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::CommandFail));
    }

    #[test]
    fn event_type_git_commit() {
        let v = EventType::GitCommit;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"git_commit\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::GitCommit));
    }

    #[test]
    fn event_type_git_checkout() {
        let v = EventType::GitCheckout;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"git_checkout\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::GitCheckout));
    }

    #[test]
    fn event_type_git_merge() {
        let v = EventType::GitMerge;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"git_merge\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::GitMerge));
    }

    #[test]
    fn event_type_build_success() {
        let v = EventType::BuildSuccess;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"build_success\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::BuildSuccess));
    }

    #[test]
    fn event_type_build_fail() {
        let v = EventType::BuildFail;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"build_fail\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::BuildFail));
    }

    #[test]
    fn event_type_error_encountered() {
        let v = EventType::ErrorEncountered;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"error_encountered\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::ErrorEncountered));
    }

    #[test]
    fn event_type_claude_session() {
        let v = EventType::ClaudeSession;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"claude_session\"");
        let out: EventType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventType::ClaudeSession));
    }

    // ---- EventSource serde roundtrip ----

    #[test]
    fn event_source_terminal() {
        let v = EventSource::Terminal;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"terminal\"");
        let out: EventSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventSource::Terminal));
    }

    #[test]
    fn event_source_filesystem() {
        let v = EventSource::Filesystem;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"filesystem\"");
        let out: EventSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventSource::Filesystem));
    }

    #[test]
    fn event_source_git() {
        let v = EventSource::Git;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"git\"");
        let out: EventSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventSource::Git));
    }

    #[test]
    fn event_source_editor() {
        let v = EventSource::Editor;
        let json = serde_json::to_string(&v).unwrap();
        assert_eq!(json, "\"editor\"");
        let out: EventSource = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, EventSource::Editor));
    }

    // ---- CortexEvent full roundtrip ----

    #[test]
    fn cortex_event_all_fields_some() {
        let event = CortexEvent {
            id: Some(42),
            timestamp: sample_timestamp(),
            event_type: EventType::FileSave,
            source: EventSource::Editor,
            project: Some("ambient-cortex".into()),
            file_path: Some("src/main.rs".into()),
            payload: serde_json::json!({"lines_changed": 10}),
            session_id: Some("sess-001".into()),
        };
        let out: CortexEvent = roundtrip(&event);
        assert_eq!(out.id, Some(42));
        assert!(matches!(out.event_type, EventType::FileSave));
        assert!(matches!(out.source, EventSource::Editor));
        assert_eq!(out.project, Some("ambient-cortex".into()));
        assert_eq!(out.file_path, Some("src/main.rs".into()));
        assert_eq!(out.payload["lines_changed"], 10);
        assert_eq!(out.session_id, Some("sess-001".into()));
    }

    #[test]
    fn cortex_event_all_optional_none() {
        let event = CortexEvent {
            id: None,
            timestamp: sample_timestamp(),
            event_type: EventType::CommandRun,
            source: EventSource::Terminal,
            project: None,
            file_path: None,
            payload: serde_json::json!(null),
            session_id: None,
        };
        let out: CortexEvent = roundtrip(&event);
        assert_eq!(out.id, None);
        assert_eq!(out.project, None);
        assert_eq!(out.file_path, None);
        assert_eq!(out.session_id, None);
        assert!(out.payload.is_null());
    }

    #[test]
    fn cortex_event_empty_payload() {
        let event = CortexEvent {
            id: None,
            timestamp: sample_timestamp(),
            event_type: EventType::GitCommit,
            source: EventSource::Git,
            project: Some("proj".into()),
            file_path: None,
            payload: serde_json::json!({}),
            session_id: None,
        };
        let out: CortexEvent = roundtrip(&event);
        assert!(out.payload.is_object());
        assert_eq!(out.payload.as_object().unwrap().len(), 0);
    }

    #[test]
    fn cortex_event_complex_payload() {
        let event = CortexEvent {
            id: Some(1),
            timestamp: sample_timestamp(),
            event_type: EventType::ErrorEncountered,
            source: EventSource::Terminal,
            project: None,
            file_path: Some("err.log".into()),
            payload: serde_json::json!({
                "message": "panic at line 42",
                "stack": ["frame1", "frame2"],
                "code": 137
            }),
            session_id: Some("s".into()),
        };
        let out: CortexEvent = roundtrip(&event);
        assert_eq!(out.payload["message"], "panic at line 42");
        assert_eq!(out.payload["stack"].as_array().unwrap().len(), 2);
        assert_eq!(out.payload["code"], 137);
    }
}
