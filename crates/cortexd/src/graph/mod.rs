pub mod embeddings;
pub mod migrations;
pub mod models;
pub mod store;

use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventType};
use cortex_common::models::{Insight, RelationType};
use cortex_common::protocol::{EventSummary, FileInfo, InsightSummary, SearchHit, SessionSummary};
use std::collections::VecDeque;
use std::sync::Mutex;

use self::embeddings::EmbeddingEngine;
use self::models::{serialize_event_type, serialize_source};
use self::store::Store;

/// Time window (in seconds) for considering files as co-edited.
const CO_EDIT_WINDOW_SECS: i64 = 300; // 5 minutes

pub struct KnowledgeGraph {
    store: Mutex<Store>,
    /// Recent file edit events for co-edit tracking: (file_path, timestamp_epoch)
    recent_edits: Mutex<VecDeque<(String, i64)>>,
    /// Optional embedding engine for semantic search
    embedding_engine: Mutex<Option<EmbeddingEngine>>,
}

impl KnowledgeGraph {
    pub fn new(store: Store) -> Self {
        Self {
            store: Mutex::new(store),
            recent_edits: Mutex::new(VecDeque::with_capacity(100)),
            embedding_engine: Mutex::new(None),
        }
    }

    pub fn init_embeddings(&self) -> Result<()> {
        let mut engine = self.embedding_engine.lock().unwrap();
        if engine.is_none() {
            tracing::info!("initializing embedding engine...");
            match EmbeddingEngine::new() {
                Ok(e) => {
                    *engine = Some(e);
                    tracing::info!("embedding engine initialized successfully");
                }
                Err(e) => {
                    tracing::warn!("failed to initialize embedding engine: {}", e);
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn ingest_event(&self, event: &CortexEvent) -> Result<()> {
        let store = self.store.lock().unwrap();
        let event_id = store.insert_event(event)?;

        // Generate and store embedding for the event summary
        let summary_text = format_event_summary(event);
        let engine = self.embedding_engine.lock().unwrap();
        if let Some(ref eng) = *engine {
            match eng.embed(&summary_text) {
                Ok(vector) => {
                    if let Err(e) = store.insert_embedding("event", event_id, &vector, &summary_text) {
                        tracing::warn!("failed to store embedding: {}", e);
                    }
                }
                Err(e) => {
                    tracing::warn!("failed to generate embedding: {}", e);
                }
            }
        }
        drop(engine);

        // Update file node if this event has a file path
        if let Some(ref file_path) = event.file_path {
            let project = event.project.as_deref().unwrap_or("unknown");
            store.upsert_file_node(file_path, project)?;
            drop(store); // Release lock before updating relations

            // Track co-edits
            let mut event_with_id = event.clone();
            event_with_id.id = Some(event_id);
            self.update_file_relations(&event_with_id)?;
        }

        Ok(())
    }

    pub fn query_file(&self, path: &str) -> Result<FileInfo> {
        let store = self.store.lock().unwrap();

        let file_node = store.get_file_node(path)?;
        let events = store.get_events_for_file(path, 20)?;
        let related = store.get_related_files(path)?;
        let pending_insights = store.get_pending_insights()?;

        let (touch_count, total_time_s, last_touched) = match &file_node {
            Some(node) => (node.touch_count, node.total_time_s, node.last_touched),
            None => (0, 0, Utc::now()),
        };

        let recent_events: Vec<EventSummary> = events
            .iter()
            .map(|e| EventSummary {
                timestamp: e.timestamp,
                event_type: serialize_event_type(&e.event_type),
                source: serialize_source(&e.source),
                summary: format_event_summary(e),
            })
            .collect();

        let related_files: Vec<String> = related.iter().map(|(p, _, _)| p.clone()).collect();

        let insights: Vec<InsightSummary> = pending_insights
            .iter()
            .filter(|i| i.file_path.as_deref() == Some(path))
            .map(|i| InsightSummary {
                title: i.title.clone(),
                body: i.body.clone(),
                relevance: i.relevance,
                insight_type: models::serialize_insight_type(&i.insight_type),
            })
            .collect();

        Ok(FileInfo {
            path: path.to_string(),
            touch_count,
            total_time_s,
            last_touched,
            related_files,
            recent_events,
            insights,
        })
    }

    pub fn get_recent_events(&self, limit: usize) -> Result<Vec<EventSummary>> {
        let store = self.store.lock().unwrap();
        let events = store.get_recent_events(limit)?;

        Ok(events
            .iter()
            .map(|e| EventSummary {
                timestamp: e.timestamp,
                event_type: serialize_event_type(&e.event_type),
                source: serialize_source(&e.source),
                summary: format_event_summary(e),
            })
            .collect())
    }

    pub fn get_stats(&self) -> Result<(i64, i64)> {
        let store = self.store.lock().unwrap();
        let event_count = store.get_event_count()?;
        let insight_count = store.get_insight_count()?;
        Ok((event_count, insight_count))
    }

    pub fn update_file_relations(&self, event: &CortexEvent) -> Result<()> {
        let file_path = match &event.file_path {
            Some(p) => p.clone(),
            None => return Ok(()),
        };

        // Only track co-edits for file-related events
        match event.event_type {
            EventType::FileSave | EventType::FileOpen => {}
            _ => return Ok(()),
        }

        let now = event.timestamp.timestamp();

        let mut recent = self.recent_edits.lock().unwrap();

        // Find files edited within the co-edit window
        let co_edited: Vec<String> = recent
            .iter()
            .filter(|(path, ts)| path != &file_path && (now - ts).abs() < CO_EDIT_WINDOW_SECS)
            .map(|(path, _)| path.clone())
            .collect();

        // Add this edit to the recent list
        recent.push_back((file_path.clone(), now));

        // Trim old entries
        while recent.len() > 100 {
            recent.pop_front();
        }
        // Also trim entries outside the window
        while let Some((_, ts)) = recent.front() {
            if (now - ts).abs() > CO_EDIT_WINDOW_SECS * 2 {
                recent.pop_front();
            } else {
                break;
            }
        }

        drop(recent);

        // Create co-edit relations
        if !co_edited.is_empty() {
            let store = self.store.lock().unwrap();
            let project = event.project.as_deref().unwrap_or("unknown");

            let file_a_id = store.upsert_file_node(&file_path, project)?;
            for co_path in &co_edited {
                let file_b_id = store.upsert_file_node(co_path, project)?;
                store.upsert_file_relation(file_a_id, file_b_id, &RelationType::CoEdited)?;
            }
        }

        Ok(())
    }

    pub fn store_insight(&self, insight: &Insight) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.insert_insight(insight)?;
        Ok(())
    }

    pub fn get_pending_insights(&self) -> Result<Vec<Insight>> {
        let store = self.store.lock().unwrap();
        store.get_pending_insights()
    }

    pub fn get_related_files(&self, path: &str) -> Result<Vec<(String, RelationType, f64)>> {
        let store = self.store.lock().unwrap();
        store.get_related_files(path)
    }

    pub fn get_events_for_file(&self, path: &str, limit: usize) -> Result<Vec<CortexEvent>> {
        let store = self.store.lock().unwrap();
        store.get_events_for_file(path, limit)
    }

    pub fn semantic_search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        let engine = self.embedding_engine.lock().unwrap();
        let eng = match engine.as_ref() {
            Some(e) => e,
            None => return Ok(Vec::new()),
        };

        let query_vector = eng.embed(query)?;
        drop(engine);

        let store = self.store.lock().unwrap();
        let results = store.search_embeddings(&query_vector, limit)?;

        Ok(results
            .into_iter()
            .map(|(_id, source_type, text, similarity)| SearchHit {
                text,
                source_type,
                relevance: similarity as f64,
            })
            .collect())
    }

    pub fn dismiss_insight(&self, id: i64) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.dismiss_insight(id)
    }

    pub fn upvote_insight(&self, id: i64) -> Result<()> {
        let store = self.store.lock().unwrap();
        store.upvote_insight(id)
    }

    pub fn get_sessions(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let store = self.store.lock().unwrap();
        store.get_sessions(limit)
    }
}

fn format_event_summary(event: &CortexEvent) -> String {
    match event.event_type {
        EventType::FileSave => {
            format!(
                "saved {}",
                event
                    .file_path
                    .as_deref()
                    .unwrap_or("unknown file")
            )
        }
        EventType::CommandRun | EventType::CommandFail => {
            let cmd = event
                .payload
                .get("cmd")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown command");
            format!("ran: {}", cmd)
        }
        EventType::GitCommit => {
            let msg = event
                .payload
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("no message");
            format!("committed: {}", msg)
        }
        EventType::GitCheckout => {
            let branch = event
                .payload
                .get("branch")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("checked out: {}", branch)
        }
        _ => serialize_event_type(&event.event_type),
    }
}
