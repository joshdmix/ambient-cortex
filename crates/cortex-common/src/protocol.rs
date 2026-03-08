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

    // ---- Request variants ----

    #[test]
    fn request_status_roundtrip() {
        let r = Request::Status;
        let out: Request = roundtrip(&r);
        assert!(matches!(out, Request::Status));
    }

    #[test]
    fn request_query_roundtrip() {
        let r = Request::Query { file_path: "src/main.rs".into() };
        let out: Request = roundtrip(&r);
        match out {
            Request::Query { file_path } => assert_eq!(file_path, "src/main.rs"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_history_roundtrip() {
        let r = Request::History { limit: 42 };
        let out: Request = roundtrip(&r);
        match out {
            Request::History { limit } => assert_eq!(limit, 42),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_search_roundtrip() {
        let r = Request::Search { query: "hello world".into() };
        let out: Request = roundtrip(&r);
        match out {
            Request::Search { query } => assert_eq!(query, "hello world"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_dismiss_insight_roundtrip() {
        let r = Request::DismissInsight { insight_id: 99 };
        let out: Request = roundtrip(&r);
        match out {
            Request::DismissInsight { insight_id } => assert_eq!(insight_id, 99),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_upvote_insight_roundtrip() {
        let r = Request::UpvoteInsight { insight_id: 7 };
        let out: Request = roundtrip(&r);
        match out {
            Request::UpvoteInsight { insight_id } => assert_eq!(insight_id, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_get_insights_roundtrip() {
        let r = Request::GetInsights;
        let out: Request = roundtrip(&r);
        assert!(matches!(out, Request::GetInsights));
    }

    #[test]
    fn request_get_sessions_roundtrip() {
        let r = Request::GetSessions { limit: 10 };
        let out: Request = roundtrip(&r);
        match out {
            Request::GetSessions { limit } => assert_eq!(limit, 10),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_get_related_files_roundtrip() {
        let r = Request::GetRelatedFiles { file_path: "/a/b.rs".into() };
        let out: Request = roundtrip(&r);
        match out {
            Request::GetRelatedFiles { file_path } => assert_eq!(file_path, "/a/b.rs"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_export_roundtrip() {
        let r = Request::Export;
        let out: Request = roundtrip(&r);
        assert!(matches!(out, Request::Export));
    }

    #[test]
    fn request_import_roundtrip() {
        let r = Request::Import { data: "{\"key\":\"val\"}".into() };
        let out: Request = roundtrip(&r);
        match out {
            Request::Import { data } => assert_eq!(data, "{\"key\":\"val\"}"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn request_shutdown_roundtrip() {
        let r = Request::Shutdown;
        let out: Request = roundtrip(&r);
        assert!(matches!(out, Request::Shutdown));
    }

    // ---- Response variants ----

    #[test]
    fn response_status_roundtrip() {
        let status = DaemonStatus {
            uptime_secs: 3600,
            event_count: 100,
            insight_count: 5,
            watchers_active: vec!["fs".into(), "git".into()],
        };
        let r = Response::Status(status);
        let out: Response = roundtrip(&r);
        match out {
            Response::Status(s) => {
                assert_eq!(s.uptime_secs, 3600);
                assert_eq!(s.event_count, 100);
                assert_eq!(s.insight_count, 5);
                assert_eq!(s.watchers_active, vec!["fs", "git"]);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_query_result_roundtrip() {
        let info = FileInfo {
            path: "src/lib.rs".into(),
            touch_count: 10,
            total_time_s: 300,
            last_touched: sample_timestamp(),
            related_files: vec!["src/main.rs".into()],
            recent_events: vec![],
            insights: vec![],
        };
        let r = Response::QueryResult(info);
        let out: Response = roundtrip(&r);
        match out {
            Response::QueryResult(f) => {
                assert_eq!(f.path, "src/lib.rs");
                assert_eq!(f.touch_count, 10);
                assert_eq!(f.total_time_s, 300);
                assert!(f.recent_events.is_empty());
                assert!(f.insights.is_empty());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_history_result_roundtrip() {
        let events = vec![EventSummary {
            timestamp: sample_timestamp(),
            event_type: "file_save".into(),
            source: "editor".into(),
            summary: "saved file".into(),
        }];
        let r = Response::HistoryResult(events);
        let out: Response = roundtrip(&r);
        match out {
            Response::HistoryResult(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].event_type, "file_save");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_history_result_empty() {
        let r = Response::HistoryResult(vec![]);
        let out: Response = roundtrip(&r);
        match out {
            Response::HistoryResult(v) => assert!(v.is_empty()),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_search_result_roundtrip() {
        let hits = vec![SearchHit {
            text: "match found".into(),
            source_type: "event".into(),
            relevance: 0.95,
        }];
        let r = Response::SearchResult(hits);
        let out: Response = roundtrip(&r);
        match out {
            Response::SearchResult(v) => {
                assert_eq!(v.len(), 1);
                assert!((v[0].relevance - 0.95).abs() < f64::EPSILON);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_insights_result_roundtrip() {
        let insights = vec![InsightSummary {
            title: "tip".into(),
            body: "do this".into(),
            relevance: 0.8,
            insight_type: "suggestion".into(),
        }];
        let r = Response::InsightsResult(insights);
        let out: Response = roundtrip(&r);
        match out {
            Response::InsightsResult(v) => assert_eq!(v.len(), 1),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_sessions_result_roundtrip() {
        let sessions = vec![SessionSummary {
            session_id: "abc-123".into(),
            start_time: sample_timestamp(),
            end_time: sample_timestamp(),
            event_count: 50,
            summary: "worked on feature".into(),
        }];
        let r = Response::SessionsResult(sessions);
        let out: Response = roundtrip(&r);
        match out {
            Response::SessionsResult(v) => {
                assert_eq!(v.len(), 1);
                assert_eq!(v[0].session_id, "abc-123");
                assert_eq!(v[0].event_count, 50);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_related_files_result_roundtrip() {
        let entries = vec![RelatedFileEntry {
            path: "test.rs".into(),
            relation: "co_edited".into(),
            strength: 0.75,
        }];
        let r = Response::RelatedFilesResult(entries);
        let out: Response = roundtrip(&r);
        match out {
            Response::RelatedFilesResult(v) => {
                assert_eq!(v.len(), 1);
                assert!((v[0].strength - 0.75).abs() < f64::EPSILON);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_export_result_roundtrip() {
        let r = Response::ExportResult("exported data".into());
        let out: Response = roundtrip(&r);
        match out {
            Response::ExportResult(s) => assert_eq!(s, "exported data"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_error_roundtrip() {
        let r = Response::Error("something broke".into());
        let out: Response = roundtrip(&r);
        match out {
            Response::Error(s) => assert_eq!(s, "something broke"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn response_ok_roundtrip() {
        let r = Response::Ok;
        let out: Response = roundtrip(&r);
        assert!(matches!(out, Response::Ok));
    }

    // ---- Data struct edge cases ----

    #[test]
    fn daemon_status_zero_values() {
        let s = DaemonStatus {
            uptime_secs: 0,
            event_count: 0,
            insight_count: 0,
            watchers_active: vec![],
        };
        let out: DaemonStatus = roundtrip(&s);
        assert_eq!(out.uptime_secs, 0);
        assert!(out.watchers_active.is_empty());
    }

    #[test]
    fn special_characters_in_strings() {
        let r = Request::Search { query: "hello \"world\" \n\ttab\\backslash".into() };
        let out: Request = roundtrip(&r);
        match out {
            Request::Search { query } => assert_eq!(query, "hello \"world\" \n\ttab\\backslash"),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn unicode_in_strings() {
        let hit = SearchHit {
            text: "emoji: \u{1F600} and CJK: \u{4E16}\u{754C}".into(),
            source_type: "test".into(),
            relevance: 1.0,
        };
        let out: SearchHit = roundtrip(&hit);
        assert!(out.text.contains('\u{1F600}'));
        assert!(out.text.contains('\u{4E16}'));
    }

    #[test]
    fn related_file_entry_roundtrip() {
        let entry = RelatedFileEntry {
            path: "/some/path".into(),
            relation: "imports".into(),
            strength: 0.0,
        };
        let out: RelatedFileEntry = roundtrip(&entry);
        assert_eq!(out.path, "/some/path");
        assert!((out.strength).abs() < f64::EPSILON);
    }

    #[test]
    fn file_info_with_nested_data() {
        let info = FileInfo {
            path: "x.rs".into(),
            touch_count: -1,
            total_time_s: 0,
            last_touched: sample_timestamp(),
            related_files: vec!["a".into(), "b".into(), "c".into()],
            recent_events: vec![EventSummary {
                timestamp: sample_timestamp(),
                event_type: "file_open".into(),
                source: "terminal".into(),
                summary: "opened".into(),
            }],
            insights: vec![InsightSummary {
                title: "t".into(),
                body: "b".into(),
                relevance: 0.5,
                insight_type: "warning".into(),
            }],
        };
        let out: FileInfo = roundtrip(&info);
        assert_eq!(out.related_files.len(), 3);
        assert_eq!(out.recent_events.len(), 1);
        assert_eq!(out.insights.len(), 1);
    }
}
