use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileNode {
    pub id: Option<i64>,
    pub path: String,
    pub project: String,
    pub first_seen: DateTime<Utc>,
    pub last_touched: DateTime<Utc>,
    pub touch_count: i64,
    pub total_time_s: i64,
    pub tags: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct FileRelation {
    pub id: Option<i64>,
    pub file_a: i64,
    pub file_b: i64,
    pub relation: RelationType,
    pub strength: f64,
    pub last_seen: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum RelationType {
    CoEdited,
    Imports,
    BreaksWhenChanged,
    TestFor,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pattern {
    pub id: Option<i64>,
    pub pattern_type: PatternType,
    pub description: String,
    pub file_paths: Vec<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub occurrence_count: i64,
    pub confidence: f64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    EditRevert,
    RepeatedError,
    DebugCycle,
    ContextSwitch,
    AlwaysCoEdit,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Insight {
    pub id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub trigger_event: Option<i64>,
    pub insight_type: InsightType,
    pub title: String,
    pub body: String,
    pub relevance: f64,
    pub surfaced: bool,
    pub dismissed: bool,
    pub file_path: Option<String>,
    pub project: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "snake_case")]
pub enum InsightType {
    Warning,
    Reminder,
    Suggestion,
    History,
}
