use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Status,
    Query { file_path: String },
    History { limit: usize },
    Search { query: String },
    DismissInsight { insight_id: i64 },
    UpvoteInsight { insight_id: i64 },
    GetInsights,
    GetSessions { limit: usize },
    GetRelatedFiles { file_path: String },
    Export,
    Import { data: String },
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Response {
    Status(DaemonStatus),
    QueryResult(FileInfo),
    HistoryResult(Vec<EventSummary>),
    SearchResult(Vec<SearchHit>),
    InsightsResult(Vec<InsightSummary>),
    SessionsResult(Vec<SessionSummary>),
    RelatedFilesResult(Vec<RelatedFileEntry>),
    ExportResult(String),
    Error(String),
    Ok,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct RelatedFileEntry {
    pub path: String,
    pub relation: String,
    pub strength: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DaemonStatus {
    pub uptime_secs: u64,
    pub event_count: u64,
    pub insight_count: u64,
    pub watchers_active: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileInfo {
    pub path: String,
    pub touch_count: i64,
    pub total_time_s: i64,
    pub last_touched: DateTime<Utc>,
    pub related_files: Vec<String>,
    pub recent_events: Vec<EventSummary>,
    pub insights: Vec<InsightSummary>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventSummary {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub source: String,
    pub summary: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InsightSummary {
    pub title: String,
    pub body: String,
    pub relevance: f64,
    pub insight_type: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SearchHit {
    pub text: String,
    pub source_type: String,
    pub relevance: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SessionSummary {
    pub session_id: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub event_count: u64,
    pub summary: String,
}
