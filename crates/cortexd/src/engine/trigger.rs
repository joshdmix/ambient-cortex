use chrono::{Duration, Utc};
use cortex_common::events::{CortexEvent, EventType};
use std::sync::Arc;

use crate::graph::KnowledgeGraph;

/// Decides which events warrant analysis by the inference engine.
pub struct TriggerEvaluator;

impl TriggerEvaluator {
    pub fn new() -> Self {
        Self
    }

    /// Returns true if the event should trigger insight generation.
    pub fn should_trigger(&self, event: &CortexEvent, graph: &Arc<KnowledgeGraph>) -> bool {
        match event.event_type {
            // Always trigger on these high-signal events
            EventType::CommandFail => true,
            EventType::BuildFail => true,
            EventType::GitCommit => true,
            EventType::GitCheckout => true,
            EventType::ErrorEncountered => true,

            // Trigger on FileSave if the file has sufficient history
            // OR if it has >2 saves in the last 5 minutes (for edit_revert_detector)
            EventType::FileSave => {
                if let Some(ref path) = event.file_path {
                    let events = graph
                        .get_events_for_file(path, 20)
                        .unwrap_or_default();

                    // Original trigger: sufficient history depth
                    if events.len() > 3 {
                        return true;
                    }

                    // New trigger: rapid saves in last 5 minutes
                    let five_min_ago = Utc::now() - Duration::minutes(5);
                    let recent_save_count = events
                        .iter()
                        .filter(|e| {
                            matches!(e.event_type, EventType::FileSave)
                                && e.timestamp > five_min_ago
                        })
                        .count();

                    recent_save_count > 2
                } else {
                    false
                }
            }

            _ => false,
        }
    }
}
