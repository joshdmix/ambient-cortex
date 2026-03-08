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
