use anyhow::Result;
use cortex_common::events::CortexEvent;
use cortex_common::models::{FileNode, Insight, RelationType};
use rusqlite::{params, Connection};
use std::path::Path;

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
            .query_map(params![path, limit as i64], |row| event_from_row(row))?
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
            .query_row(params![path], |row| file_node_from_row(row))
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
            .query_map([], |row| insight_from_row(row))?
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
            .query_map(params![limit as i64], |row| event_from_row(row))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(events)
    }
}
