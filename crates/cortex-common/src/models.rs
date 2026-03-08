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

    // ---- FileNode ----

    #[test]
    fn file_node_roundtrip() {
        let node = FileNode {
            id: Some(1),
            path: "src/lib.rs".into(),
            project: "cortex".into(),
            first_seen: sample_timestamp(),
            last_touched: sample_timestamp(),
            touch_count: 5,
            total_time_s: 120,
            tags: vec!["rust".into(), "lib".into()],
        };
        let out: FileNode = roundtrip(&node);
        assert_eq!(out.id, Some(1));
        assert_eq!(out.path, "src/lib.rs");
        assert_eq!(out.project, "cortex");
        assert_eq!(out.touch_count, 5);
        assert_eq!(out.total_time_s, 120);
        assert_eq!(out.tags, vec!["rust", "lib"]);
    }

    #[test]
    fn file_node_none_id_empty_tags() {
        let node = FileNode {
            id: None,
            path: "x".into(),
            project: "p".into(),
            first_seen: sample_timestamp(),
            last_touched: sample_timestamp(),
            touch_count: 0,
            total_time_s: 0,
            tags: vec![],
        };
        let out: FileNode = roundtrip(&node);
        assert_eq!(out.id, None);
        assert!(out.tags.is_empty());
        assert_eq!(out.touch_count, 0);
    }

    // ---- FileRelation ----

    #[test]
    fn file_relation_roundtrip() {
        let rel = FileRelation {
            id: Some(10),
            file_a: 1,
            file_b: 2,
            relation: RelationType::CoEdited,
            strength: 0.9,
            last_seen: sample_timestamp(),
        };
        let out: FileRelation = roundtrip(&rel);
        assert_eq!(out.id, Some(10));
        assert_eq!(out.file_a, 1);
        assert_eq!(out.file_b, 2);
        assert!(matches!(out.relation, RelationType::CoEdited));
        assert!((out.strength - 0.9).abs() < f64::EPSILON);
    }

    #[test]
    fn file_relation_none_id() {
        let rel = FileRelation {
            id: None,
            file_a: 0,
            file_b: 0,
            relation: RelationType::Imports,
            strength: 0.0,
            last_seen: sample_timestamp(),
        };
        let out: FileRelation = roundtrip(&rel);
        assert_eq!(out.id, None);
        assert!((out.strength).abs() < f64::EPSILON);
    }

    // ---- RelationType ----

    #[test]
    fn relation_type_co_edited() {
        let json = serde_json::to_string(&RelationType::CoEdited).unwrap();
        assert_eq!(json, "\"co_edited\"");
        let out: RelationType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, RelationType::CoEdited));
    }

    #[test]
    fn relation_type_imports() {
        let json = serde_json::to_string(&RelationType::Imports).unwrap();
        assert_eq!(json, "\"imports\"");
        let out: RelationType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, RelationType::Imports));
    }

    #[test]
    fn relation_type_breaks_when_changed() {
        let json = serde_json::to_string(&RelationType::BreaksWhenChanged).unwrap();
        assert_eq!(json, "\"breaks_when_changed\"");
        let out: RelationType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, RelationType::BreaksWhenChanged));
    }

    #[test]
    fn relation_type_test_for() {
        let json = serde_json::to_string(&RelationType::TestFor).unwrap();
        assert_eq!(json, "\"test_for\"");
        let out: RelationType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, RelationType::TestFor));
    }

    // ---- Pattern ----

    #[test]
    fn pattern_roundtrip() {
        let p = Pattern {
            id: Some(3),
            pattern_type: PatternType::EditRevert,
            description: "reverted edit".into(),
            file_paths: vec!["a.rs".into(), "b.rs".into()],
            first_seen: sample_timestamp(),
            last_seen: sample_timestamp(),
            occurrence_count: 7,
            confidence: 0.85,
        };
        let out: Pattern = roundtrip(&p);
        assert_eq!(out.id, Some(3));
        assert!(matches!(out.pattern_type, PatternType::EditRevert));
        assert_eq!(out.description, "reverted edit");
        assert_eq!(out.file_paths.len(), 2);
        assert_eq!(out.occurrence_count, 7);
        assert!((out.confidence - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn pattern_none_id_empty_paths() {
        let p = Pattern {
            id: None,
            pattern_type: PatternType::ContextSwitch,
            description: "".into(),
            file_paths: vec![],
            first_seen: sample_timestamp(),
            last_seen: sample_timestamp(),
            occurrence_count: 0,
            confidence: 0.0,
        };
        let out: Pattern = roundtrip(&p);
        assert_eq!(out.id, None);
        assert!(out.file_paths.is_empty());
        assert_eq!(out.description, "");
    }

    // ---- PatternType ----

    #[test]
    fn pattern_type_edit_revert() {
        let json = serde_json::to_string(&PatternType::EditRevert).unwrap();
        assert_eq!(json, "\"edit_revert\"");
        let out: PatternType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, PatternType::EditRevert));
    }

    #[test]
    fn pattern_type_repeated_error() {
        let json = serde_json::to_string(&PatternType::RepeatedError).unwrap();
        assert_eq!(json, "\"repeated_error\"");
        let out: PatternType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, PatternType::RepeatedError));
    }

    #[test]
    fn pattern_type_debug_cycle() {
        let json = serde_json::to_string(&PatternType::DebugCycle).unwrap();
        assert_eq!(json, "\"debug_cycle\"");
        let out: PatternType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, PatternType::DebugCycle));
    }

    #[test]
    fn pattern_type_context_switch() {
        let json = serde_json::to_string(&PatternType::ContextSwitch).unwrap();
        assert_eq!(json, "\"context_switch\"");
        let out: PatternType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, PatternType::ContextSwitch));
    }

    #[test]
    fn pattern_type_always_co_edit() {
        let json = serde_json::to_string(&PatternType::AlwaysCoEdit).unwrap();
        assert_eq!(json, "\"always_co_edit\"");
        let out: PatternType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, PatternType::AlwaysCoEdit));
    }

    // ---- Insight ----

    #[test]
    fn insight_roundtrip_all_fields() {
        let i = Insight {
            id: Some(5),
            created_at: sample_timestamp(),
            trigger_event: Some(100),
            insight_type: InsightType::Warning,
            title: "watch out".into(),
            body: "this file breaks often".into(),
            relevance: 0.95,
            surfaced: true,
            dismissed: false,
            file_path: Some("danger.rs".into()),
            project: Some("proj".into()),
        };
        let out: Insight = roundtrip(&i);
        assert_eq!(out.id, Some(5));
        assert_eq!(out.trigger_event, Some(100));
        assert!(matches!(out.insight_type, InsightType::Warning));
        assert_eq!(out.title, "watch out");
        assert!(out.surfaced);
        assert!(!out.dismissed);
        assert_eq!(out.file_path, Some("danger.rs".into()));
        assert_eq!(out.project, Some("proj".into()));
    }

    #[test]
    fn insight_all_optional_none() {
        let i = Insight {
            id: None,
            created_at: sample_timestamp(),
            trigger_event: None,
            insight_type: InsightType::Suggestion,
            title: "".into(),
            body: "".into(),
            relevance: 0.0,
            surfaced: false,
            dismissed: false,
            file_path: None,
            project: None,
        };
        let out: Insight = roundtrip(&i);
        assert_eq!(out.id, None);
        assert_eq!(out.trigger_event, None);
        assert_eq!(out.file_path, None);
        assert_eq!(out.project, None);
    }

    // ---- InsightType ----

    #[test]
    fn insight_type_warning() {
        let json = serde_json::to_string(&InsightType::Warning).unwrap();
        assert_eq!(json, "\"warning\"");
        let out: InsightType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, InsightType::Warning));
    }

    #[test]
    fn insight_type_reminder() {
        let json = serde_json::to_string(&InsightType::Reminder).unwrap();
        assert_eq!(json, "\"reminder\"");
        let out: InsightType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, InsightType::Reminder));
    }

    #[test]
    fn insight_type_suggestion() {
        let json = serde_json::to_string(&InsightType::Suggestion).unwrap();
        assert_eq!(json, "\"suggestion\"");
        let out: InsightType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, InsightType::Suggestion));
    }

    #[test]
    fn insight_type_history() {
        let json = serde_json::to_string(&InsightType::History).unwrap();
        assert_eq!(json, "\"history\"");
        let out: InsightType = serde_json::from_str(&json).unwrap();
        assert!(matches!(out, InsightType::History));
    }
}
