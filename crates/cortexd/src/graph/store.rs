use anyhow::Result;
use chrono::{DateTime, Utc};
use cortex_common::events::CortexEvent;
use cortex_common::models::{FileNode, Insight, RelationType};
use cortex_common::protocol::SessionSummary;
use rusqlite::{params, Connection};
use std::path::Path;

use super::embeddings::EmbeddingEngine;
use super::migrations;
use super::models::{
    event_from_row, file_node_from_row, insight_from_row, serialize_event_type,
    serialize_insight_type, serialize_relation_type, serialize_source,
};

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn new(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")?;
        migrations::run_migrations(&conn)?;
        Ok(Self { conn })
    }

    pub fn insert_event(&self, event: &CortexEvent) -> Result<i64> {
        let event_type = serialize_event_type(&event.event_type);
        let source = serialize_source(&event.source);
        let payload = serde_json::to_string(&event.payload)?;
        let timestamp = event.timestamp.to_rfc3339();

        self.conn.execute(
            "INSERT INTO events (timestamp, event_type, source, project, file_path, payload, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                timestamp,
                event_type,
                source,
                event.project,
                event.file_path,
                payload,
                event.session_id,
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_events_for_file(&self, path: &str, limit: usize) -> Result<Vec<CortexEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, event_type, source, project, file_path, payload, session_id
             FROM events WHERE file_path = ?1 ORDER BY timestamp DESC LIMIT ?2",
        )?;

        let events = stmt
            .query_map(params![path, limit as i64], event_from_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(events)
    }

    pub fn upsert_file_node(&self, path: &str, project: &str) -> Result<i64> {
        let now = chrono::Utc::now().to_rfc3339();

        self.conn.execute(
            "INSERT INTO file_nodes (path, project, first_seen, last_touched, touch_count)
             VALUES (?1, ?2, ?3, ?3, 1)
             ON CONFLICT(path) DO UPDATE SET
                last_touched = ?3,
                touch_count = touch_count + 1",
            params![path, project, now],
        )?;

        let id: i64 = self.conn.query_row(
            "SELECT id FROM file_nodes WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;

        Ok(id)
    }

    pub fn get_file_node(&self, path: &str) -> Result<Option<FileNode>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, path, project, first_seen, last_touched, touch_count, total_time_s, tags
             FROM file_nodes WHERE path = ?1",
        )?;

        let result = stmt
            .query_row(params![path], file_node_from_row)
            .ok();

        Ok(result)
    }

    pub fn upsert_file_relation(
        &self,
        file_a: i64,
        file_b: i64,
        relation: &RelationType,
    ) -> Result<()> {
        let now = chrono::Utc::now().to_rfc3339();
        let relation_str = serialize_relation_type(relation);

        // Ensure consistent ordering (smaller id first)
        let (a, b) = if file_a <= file_b {
            (file_a, file_b)
        } else {
            (file_b, file_a)
        };

        self.conn.execute(
            "INSERT INTO file_relations (file_a, file_b, relation, strength, last_seen)
             VALUES (?1, ?2, ?3, 1.0, ?4)
             ON CONFLICT(file_a, file_b, relation) DO UPDATE SET
                strength = strength + 1.0,
                last_seen = ?4",
            params![a, b, relation_str, now],
        )?;

        Ok(())
    }

    pub fn get_related_files(&self, path: &str) -> Result<Vec<(String, RelationType, f64)>> {
        // First get the file node id
        let file_id: Option<i64> = self
            .conn
            .query_row(
                "SELECT id FROM file_nodes WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .ok();

        let file_id = match file_id {
            Some(id) => id,
            None => return Ok(Vec::new()),
        };

        let mut stmt = self.conn.prepare(
            "SELECT fn2.path, fr.relation, fr.strength
             FROM file_relations fr
             JOIN file_nodes fn2 ON (
                 CASE WHEN fr.file_a = ?1 THEN fr.file_b ELSE fr.file_a END = fn2.id
             )
             WHERE fr.file_a = ?1 OR fr.file_b = ?1
             ORDER BY fr.strength DESC",
        )?;

        let results = stmt
            .query_map(params![file_id], |row| {
                let path: String = row.get(0)?;
                let relation_str: String = row.get(1)?;
                let strength: f64 = row.get(2)?;
                let relation = match relation_str.as_str() {
                    "co_edited" => RelationType::CoEdited,
                    "imports" => RelationType::Imports,
                    "breaks_when_changed" => RelationType::BreaksWhenChanged,
                    "test_for" => RelationType::TestFor,
                    _ => RelationType::CoEdited,
                };
                Ok((path, relation, strength))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(results)
    }

    pub fn insert_insight(&self, insight: &Insight) -> Result<()> {
        let created_at = insight.created_at.to_rfc3339();
        let insight_type = serialize_insight_type(&insight.insight_type);

        self.conn.execute(
            "INSERT INTO insights (created_at, trigger_event, insight_type, title, body, relevance, surfaced, dismissed, file_path, project)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                created_at,
                insight.trigger_event,
                insight_type,
                insight.title,
                insight.body,
                insight.relevance,
                insight.surfaced as i64,
                insight.dismissed as i64,
                insight.file_path,
                insight.project,
            ],
        )?;

        Ok(())
    }

    pub fn get_pending_insights(&self) -> Result<Vec<Insight>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, created_at, trigger_event, insight_type, title, body, relevance, surfaced, dismissed, file_path, project
             FROM insights WHERE surfaced = 0 AND dismissed = 0
             ORDER BY relevance DESC",
        )?;

        let insights = stmt
            .query_map([], insight_from_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(insights)
    }

    pub fn get_event_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_insight_count(&self) -> Result<i64> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM insights", [], |row| row.get(0))?;
        Ok(count)
    }

    pub fn get_recent_events(&self, limit: usize) -> Result<Vec<CortexEvent>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, event_type, source, project, file_path, payload, session_id
             FROM events ORDER BY timestamp DESC LIMIT ?1",
        )?;

        let events = stmt
            .query_map(params![limit as i64], event_from_row)?
            .filter_map(|r| r.ok())
            .collect();

        Ok(events)
    }

    pub fn dismiss_insight(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE insights SET dismissed = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn upvote_insight(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE insights SET relevance = MIN(relevance + 0.1, 1.0), surfaced = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn mark_insight_surfaced(&self, id: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE insights SET surfaced = 1 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn insert_embedding(
        &self,
        source_type: &str,
        source_id: i64,
        vector: &[f32],
        text: &str,
    ) -> Result<()> {
        let vector_bytes: Vec<u8> = vector
            .iter()
            .flat_map(|f| f.to_le_bytes())
            .collect();

        self.conn.execute(
            "INSERT INTO embeddings (source_type, source_id, vector, text) VALUES (?1, ?2, ?3, ?4)",
            params![source_type, source_id, vector_bytes, text],
        )?;

        Ok(())
    }

    pub fn search_embeddings(
        &self,
        query_vector: &[f32],
        limit: usize,
    ) -> Result<Vec<(i64, String, String, f32)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, source_type, vector, text FROM embeddings",
        )?;

        let mut results: Vec<(i64, String, String, f32)> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let source_type: String = row.get(1)?;
                let vector_bytes: Vec<u8> = row.get(2)?;
                let text: String = row.get(3)?;
                Ok((id, source_type, vector_bytes, text))
            })?
            .filter_map(|r| r.ok())
            .map(|(id, source_type, vector_bytes, text)| {
                let stored_vector: Vec<f32> = vector_bytes
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect();
                let similarity = EmbeddingEngine::cosine_similarity(query_vector, &stored_vector);
                (id, source_type, text, similarity)
            })
            .collect();

        results.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(limit);

        Ok(results)
    }

    pub fn get_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT session_id, MIN(timestamp) as start_time, MAX(timestamp) as end_time, COUNT(*) as event_count
             FROM events
             WHERE session_id IS NOT NULL
             GROUP BY session_id
             ORDER BY start_time DESC
             LIMIT ?1",
        )?;

        let sessions = stmt
            .query_map(params![limit as i64], |row| {
                let session_id: String = row.get(0)?;
                let start_str: String = row.get(1)?;
                let end_str: String = row.get(2)?;
                let event_count: i64 = row.get(3)?;
                Ok((session_id, start_str, end_str, event_count))
            })?
            .filter_map(|r| r.ok())
            .map(|(session_id, start_str, end_str, event_count)| {
                let start_time: DateTime<Utc> =
                    start_str.parse().unwrap_or_else(|_| Utc::now());
                let end_time: DateTime<Utc> =
                    end_str.parse().unwrap_or_else(|_| Utc::now());
                SessionSummary {
                    session_id,
                    start_time,
                    end_time,
                    event_count: event_count as u64,
                    summary: format!("{} events", event_count),
                }
            })
            .collect();

        Ok(sessions)
    }

    pub fn prune_old_events(&self, retention_days: u64) -> Result<u64> {
        let cutoff = (chrono::Utc::now() - chrono::Duration::days(retention_days as i64)).to_rfc3339();
        let deleted = self.conn.execute(
            "DELETE FROM events WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(deleted as u64)
    }

    pub fn prune_orphaned_embeddings(&self) -> Result<u64> {
        let deleted = self.conn.execute(
            "DELETE FROM embeddings WHERE source_type = 'event' AND source_id NOT IN (SELECT id FROM events)",
            [],
        )?;
        Ok(deleted as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use cortex_common::events::{CortexEvent, EventSource, EventType};
    use cortex_common::models::{Insight, InsightType, RelationType};
    use tempfile::NamedTempFile;

    fn make_event(
        file_path: Option<&str>,
        event_type: EventType,
        session_id: Option<&str>,
    ) -> CortexEvent {
        CortexEvent {
            id: None,
            timestamp: Utc::now(),
            event_type,
            source: EventSource::Editor,
            project: Some("test-project".to_string()),
            file_path: file_path.map(|s| s.to_string()),
            payload: serde_json::json!({"test": true}),
            session_id: session_id.map(|s| s.to_string()),
        }
    }

    fn make_event_with_timestamp(
        file_path: Option<&str>,
        timestamp: chrono::DateTime<Utc>,
    ) -> CortexEvent {
        CortexEvent {
            id: None,
            timestamp,
            event_type: EventType::FileSave,
            source: EventSource::Editor,
            project: Some("test-project".to_string()),
            file_path: file_path.map(|s| s.to_string()),
            payload: serde_json::json!({}),
            session_id: None,
        }
    }

    fn make_insight(title: &str, relevance: f64) -> Insight {
        Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: None,
            insight_type: InsightType::Suggestion,
            title: title.to_string(),
            body: "test body".to_string(),
            relevance,
            surfaced: false,
            dismissed: false,
            file_path: Some("/test/file.rs".to_string()),
            project: Some("test-project".to_string()),
        }
    }

    fn new_store() -> (Store, NamedTempFile) {
        let tmp = NamedTempFile::new().unwrap();
        let store = Store::new(tmp.path()).unwrap();
        (store, tmp)
    }

    #[test]
    fn insert_event_and_get_events_for_file() {
        let (store, _tmp) = new_store();

        let e1 = make_event_with_timestamp(
            Some("/src/main.rs"),
            Utc::now() - Duration::seconds(10),
        );
        let e2 = make_event_with_timestamp(
            Some("/src/main.rs"),
            Utc::now(),
        );
        let e3 = make_event(Some("/src/other.rs"), EventType::FileOpen, None);

        store.insert_event(&e1).unwrap();
        store.insert_event(&e2).unwrap();
        store.insert_event(&e3).unwrap();

        let events = store.get_events_for_file("/src/main.rs", 10).unwrap();
        assert_eq!(events.len(), 2);
        // Should be DESC by timestamp — newest first
        assert!(events[0].timestamp >= events[1].timestamp);

        // Other file should not appear
        let other = store.get_events_for_file("/src/other.rs", 10).unwrap();
        assert_eq!(other.len(), 1);
    }

    #[test]
    fn insert_event_and_get_recent_events() {
        let (store, _tmp) = new_store();

        for i in 0..5 {
            let e = make_event_with_timestamp(
                Some(&format!("/file{}.rs", i)),
                Utc::now() - Duration::seconds(5 - i),
            );
            store.insert_event(&e).unwrap();
        }

        let recent = store.get_recent_events(3).unwrap();
        assert_eq!(recent.len(), 3);
        // Verify DESC ordering
        assert!(recent[0].timestamp >= recent[1].timestamp);
        assert!(recent[1].timestamp >= recent[2].timestamp);

        let all = store.get_recent_events(100).unwrap();
        assert_eq!(all.len(), 5);
    }

    #[test]
    fn upsert_file_node_increments_touch_count() {
        let (store, _tmp) = new_store();

        let id1 = store.upsert_file_node("/src/lib.rs", "proj").unwrap();
        let node = store.get_file_node("/src/lib.rs").unwrap().unwrap();
        assert_eq!(node.touch_count, 1);

        let id2 = store.upsert_file_node("/src/lib.rs", "proj").unwrap();
        assert_eq!(id1, id2);
        let node = store.get_file_node("/src/lib.rs").unwrap().unwrap();
        assert_eq!(node.touch_count, 2);

        let id3 = store.upsert_file_node("/src/lib.rs", "proj").unwrap();
        assert_eq!(id1, id3);
        let node = store.get_file_node("/src/lib.rs").unwrap().unwrap();
        assert_eq!(node.touch_count, 3);
    }

    #[test]
    fn get_file_node_existing_and_missing() {
        let (store, _tmp) = new_store();

        assert!(store.get_file_node("/nonexistent").unwrap().is_none());

        store.upsert_file_node("/exists.rs", "proj").unwrap();
        let node = store.get_file_node("/exists.rs").unwrap();
        assert!(node.is_some());
        let node = node.unwrap();
        assert_eq!(node.path, "/exists.rs");
        assert_eq!(node.project, "proj");
    }

    #[test]
    fn upsert_file_relation_and_get_related_files() {
        let (store, _tmp) = new_store();

        let id_a = store.upsert_file_node("/a.rs", "proj").unwrap();
        let id_b = store.upsert_file_node("/b.rs", "proj").unwrap();
        let id_c = store.upsert_file_node("/c.rs", "proj").unwrap();

        store
            .upsert_file_relation(id_a, id_b, &RelationType::CoEdited)
            .unwrap();
        store
            .upsert_file_relation(id_a, id_c, &RelationType::Imports)
            .unwrap();

        let related = store.get_related_files("/a.rs").unwrap();
        assert_eq!(related.len(), 2);

        // Upsert again to increment strength
        store
            .upsert_file_relation(id_a, id_b, &RelationType::CoEdited)
            .unwrap();
        let related = store.get_related_files("/a.rs").unwrap();
        let co_edited = related.iter().find(|(p, _, _)| p == "/b.rs").unwrap();
        assert!((co_edited.2 - 2.0).abs() < f64::EPSILON, "strength should be 2.0 after re-upsert");

        // Query from the other side too
        let related_b = store.get_related_files("/b.rs").unwrap();
        assert!(!related_b.is_empty());
    }

    #[test]
    fn insert_insight_and_get_pending() {
        let (store, _tmp) = new_store();

        store.insert_insight(&make_insight("insight1", 0.8)).unwrap();
        store.insert_insight(&make_insight("insight2", 0.5)).unwrap();

        // Insert one that is already surfaced
        let mut surfaced = make_insight("surfaced", 0.9);
        surfaced.surfaced = true;
        store.insert_insight(&surfaced).unwrap();

        // Insert one that is already dismissed
        let mut dismissed = make_insight("dismissed", 0.7);
        dismissed.dismissed = true;
        store.insert_insight(&dismissed).unwrap();

        let pending = store.get_pending_insights().unwrap();
        assert_eq!(pending.len(), 2);
        // Ordered by relevance DESC
        assert!(pending[0].relevance >= pending[1].relevance);
        assert_eq!(pending[0].title, "insight1");
        assert_eq!(pending[1].title, "insight2");
    }

    #[test]
    fn dismiss_insight_filters_from_pending() {
        let (store, _tmp) = new_store();

        store.insert_insight(&make_insight("to_dismiss", 0.8)).unwrap();
        store.insert_insight(&make_insight("keep", 0.5)).unwrap();

        let pending = store.get_pending_insights().unwrap();
        assert_eq!(pending.len(), 2);
        let dismiss_id = pending.iter().find(|i| i.title == "to_dismiss").unwrap().id.unwrap();

        store.dismiss_insight(dismiss_id).unwrap();

        let pending = store.get_pending_insights().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].title, "keep");
    }

    #[test]
    fn upvote_insight_increases_relevance_and_surfaces() {
        let (store, _tmp) = new_store();

        store.insert_insight(&make_insight("upvotable", 0.5)).unwrap();

        let pending = store.get_pending_insights().unwrap();
        let id = pending[0].id.unwrap();
        let original_relevance = pending[0].relevance;

        store.upvote_insight(id).unwrap();

        // After upvote it's surfaced, so not in pending
        let pending = store.get_pending_insights().unwrap();
        assert!(pending.is_empty());

        // Verify relevance increased by querying directly
        let count: f64 = store
            .conn
            .query_row(
                "SELECT relevance FROM insights WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert!((count - (original_relevance + 0.1)).abs() < f64::EPSILON);

        // Verify surfaced flag
        let surfaced: i64 = store
            .conn
            .query_row(
                "SELECT surfaced FROM insights WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(surfaced, 1);
    }

    #[test]
    fn mark_insight_surfaced_sets_flag() {
        let (store, _tmp) = new_store();

        store.insert_insight(&make_insight("mark_me", 0.5)).unwrap();
        let pending = store.get_pending_insights().unwrap();
        assert_eq!(pending.len(), 1);
        let id = pending[0].id.unwrap();

        store.mark_insight_surfaced(id).unwrap();

        // Should no longer be pending
        let pending = store.get_pending_insights().unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn get_event_count_and_get_insight_count() {
        let (store, _tmp) = new_store();

        assert_eq!(store.get_event_count().unwrap(), 0);
        assert_eq!(store.get_insight_count().unwrap(), 0);

        for _ in 0..3 {
            store
                .insert_event(&make_event(Some("/f.rs"), EventType::FileSave, None))
                .unwrap();
        }
        store.insert_insight(&make_insight("i1", 0.5)).unwrap();
        store.insert_insight(&make_insight("i2", 0.6)).unwrap();

        assert_eq!(store.get_event_count().unwrap(), 3);
        assert_eq!(store.get_insight_count().unwrap(), 2);
    }

    #[test]
    fn get_sessions_groups_by_session_id() {
        let (store, _tmp) = new_store();

        // Session A: 2 events
        store
            .insert_event(&make_event(Some("/a.rs"), EventType::FileSave, Some("sess-a")))
            .unwrap();
        store
            .insert_event(&make_event(Some("/b.rs"), EventType::FileOpen, Some("sess-a")))
            .unwrap();

        // Session B: 1 event
        store
            .insert_event(&make_event(Some("/c.rs"), EventType::FileSave, Some("sess-b")))
            .unwrap();

        // No session: should not appear
        store
            .insert_event(&make_event(Some("/d.rs"), EventType::FileSave, None))
            .unwrap();

        let sessions = store.get_sessions(10).unwrap();
        assert_eq!(sessions.len(), 2);

        let sess_a = sessions.iter().find(|s| s.session_id == "sess-a").unwrap();
        assert_eq!(sess_a.event_count, 2);

        let sess_b = sessions.iter().find(|s| s.session_id == "sess-b").unwrap();
        assert_eq!(sess_b.event_count, 1);
    }

    #[test]
    fn prune_old_events_deletes_only_old() {
        let (store, _tmp) = new_store();

        // Old event: 100 days ago
        let old = make_event_with_timestamp(
            Some("/old.rs"),
            Utc::now() - Duration::days(100),
        );
        // Recent event: now
        let recent = make_event_with_timestamp(Some("/new.rs"), Utc::now());

        store.insert_event(&old).unwrap();
        store.insert_event(&recent).unwrap();
        assert_eq!(store.get_event_count().unwrap(), 2);

        let deleted = store.prune_old_events(30).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(store.get_event_count().unwrap(), 1);

        let remaining = store.get_recent_events(10).unwrap();
        assert_eq!(remaining[0].file_path.as_deref(), Some("/new.rs"));
    }

    #[test]
    fn prune_orphaned_embeddings_cleans_up() {
        let (store, _tmp) = new_store();

        // Insert an event and its embedding
        let event_id = store
            .insert_event(&make_event(Some("/f.rs"), EventType::FileSave, None))
            .unwrap();
        store
            .insert_embedding("event", event_id, &[0.1, 0.2, 0.3], "test text")
            .unwrap();

        // Insert an embedding for a non-existent event
        store
            .insert_embedding("event", 9999, &[0.4, 0.5, 0.6], "orphan text")
            .unwrap();

        let count_before: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_before, 2);

        let pruned = store.prune_orphaned_embeddings().unwrap();
        assert_eq!(pruned, 1);

        let count_after: i64 = store
            .conn
            .query_row("SELECT COUNT(*) FROM embeddings", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count_after, 1);
    }
}
