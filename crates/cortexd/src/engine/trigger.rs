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
            EventType::GitCommit => true,
            EventType::GitCheckout => true,
            EventType::ErrorEncountered => true,

            // Trigger on FileSave if the file has sufficient history
            EventType::FileSave => {
                if let Some(ref path) = event.file_path {
                    let event_count = graph
                        .get_events_for_file(path, 4)
                        .map(|events| events.len())
                        .unwrap_or(0);
                    event_count > 3
                } else {
                    false
                }
            }

            _ => false,
        }
    }
}
