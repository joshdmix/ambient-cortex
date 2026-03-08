use chrono::{Duration, Utc};
use cortex_common::events::{CortexEvent, EventType};
use cortex_common::models::{Insight, InsightType};
use std::sync::Arc;

use crate::graph::KnowledgeGraph;

/// Local (Tier 1) rules that generate insights without API calls.
pub struct LocalRules;

impl LocalRules {
    pub fn new() -> Self {
        Self
    }

    /// Run all local rules against the event. Returns any generated insights.
    pub fn evaluate(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Vec<Insight> {
        let mut insights = Vec::new();

        if let Some(insight) = self.co_edit_reminder(event, graph) {
            insights.push(insight);
        }

        if let Some(insight) = self.context_switch(event, graph) {
            insights.push(insight);
        }

        if let Some(insight) = self.edit_revert_detector(event, graph) {
            insights.push(insight);
        }

        if let Some(insight) = self.error_pattern(event, graph) {
            insights.push(insight);
        }

        if let Some(insight) = self.long_debug_cycle(event, graph) {
            insights.push(insight);
        }

        insights
    }

    /// When a file is saved, check for strong co-edit pairs.
    fn co_edit_reminder(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        let file_path = event.file_path.as_ref()?;

        let related = graph.get_related_files(file_path).ok()?;

        // Find strong co-edit relations (strength > 3.0)
        let strong_pairs: Vec<&str> = related
            .iter()
            .filter(|(_, rel, strength)| {
                matches!(rel, cortex_common::models::RelationType::CoEdited) && *strength > 3.0
            })
            .map(|(path, _, _)| path.as_str())
            .collect();

        if strong_pairs.is_empty() {
            return None;
        }

        let files_list = strong_pairs.join(", ");
        let title = format!("You usually edit these files together");
        let body = format!(
            "When you change {}, you typically also change: {}",
            file_path, files_list
        );

        Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: event.id,
            insight_type: InsightType::Reminder,
            title,
            body,
            relevance: 0.7,
            surfaced: false,
            dismissed: false,
            file_path: Some(file_path.clone()),
            project: event.project.clone(),
        })
    }

    /// When the project changes between events, generate a context switch insight.
    fn context_switch(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        let current_project = event.project.as_ref()?;

        // Get the most recent events to detect project change
        let recent = graph.get_recent_events(5).ok()?;
        if recent.len() < 2 {
            return None;
        }

        // Check if we just switched projects by looking at recent events
        // The most recent event before this one would be at index 0
        // (since we just ingested the current event, it should be there)
        // But we need to check if the previous events were in a different project.
        // Since recent_events returns EventSummary which doesn't have project,
        // we'll use a simpler approach: check recent file events for this project.
        let project_events = graph
            .get_recent_events(20)
            .ok()?;

        // If we have events, check if this project hadn't appeared in recent history
        // This is a simplified heuristic
        if project_events.len() < 5 {
            return None;
        }

        // Look at the last few event summaries - if none mention this project's files,
        // it's likely a context switch. For now, we detect this by checking if
        // the project has events from more than a threshold ago.
        // Simplified: just check if there were recent events for this project.
        // A full implementation would track project per event, but EventSummary
        // doesn't carry project info. We'll do a basic check via the graph.

        // For MVP, only fire if we can detect the project had older activity
        let file_path = event.file_path.as_ref()?;
        let file_events = graph.get_events_for_file(file_path, 10).ok()?;

        if file_events.len() < 2 {
            return None;
        }

        // Check if there's a gap of more than 1 hour since last event for this file
        let last_event = &file_events[0]; // most recent (which is the current one)
        if file_events.len() >= 2 {
            let prev_event = &file_events[1];
            let gap = (last_event.timestamp - prev_event.timestamp).num_seconds();
            if gap > 3600 {
                // More than 1 hour gap
                let title = "Welcome back to this project".to_string();
                let body = format!(
                    "Last activity in {} was {} ago. Previous work involved: {}",
                    current_project,
                    format_duration(gap),
                    prev_event
                        .file_path
                        .as_deref()
                        .unwrap_or("unknown files")
                );

                return Some(Insight {
                    id: None,
                    created_at: Utc::now(),
                    trigger_event: event.id,
                    insight_type: InsightType::History,
                    title,
                    body,
                    relevance: 0.6,
                    surfaced: false,
                    dismissed: false,
                    file_path: event.file_path.clone(),
                    project: Some(current_project.clone()),
                });
            }
        }

        None
    }

    /// Detect rapid file saves suggesting an iterative debug cycle.
    /// If the same file has >3 saves within 5 minutes, generate a warning.
    fn edit_revert_detector(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        if !matches!(event.event_type, EventType::FileSave) {
            return None;
        }

        let file_path = event.file_path.as_ref()?;
        let file_events = graph.get_events_for_file(file_path, 20).ok()?;

        let five_min_ago = Utc::now() - Duration::minutes(5);

        let save_count = file_events
            .iter()
            .filter(|e| {
                matches!(e.event_type, EventType::FileSave) && e.timestamp > five_min_ago
            })
            .count();

        if save_count <= 3 {
            return None;
        }

        let title = "Rapid file saves detected".to_string();
        let body = format!(
            "You've saved {} {} times in 5 minutes — possible debug cycle.",
            file_path, save_count
        );

        Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: event.id,
            insight_type: InsightType::Warning,
            title,
            body,
            relevance: 0.75,
            surfaced: false,
            dismissed: false,
            file_path: Some(file_path.clone()),
            project: event.project.clone(),
        })
    }

    /// Detect repeated command failures of the same type.
    /// If the same command prefix has failed >2 times in the last 30 minutes, generate a warning.
    fn error_pattern(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        if !matches!(event.event_type, EventType::CommandFail) {
            return None;
        }

        // Extract command prefix from the current event's payload
        let cmd = event
            .payload
            .get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let cmd_prefix = extract_command_prefix(cmd);
        if cmd_prefix.is_empty() {
            return None;
        }

        let thirty_min_ago = Utc::now() - Duration::minutes(30);

        // Use EventSummary-based recent events to find similar failures
        let recent = graph.get_recent_events(100).ok()?;

        let fail_count = recent
            .iter()
            .filter(|e| {
                e.event_type == "command_fail"
                    && e.timestamp > thirty_min_ago
                    && e.summary.contains(&cmd_prefix)
            })
            .count();

        if fail_count <= 2 {
            return None;
        }

        let title = "Repeated command failures".to_string();
        let body = format!(
            "This command has failed {} times recently. Consider checking: [files from recent git commits].",
            fail_count
        );

        Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: event.id,
            insight_type: InsightType::Warning,
            title,
            body,
            relevance: 0.85,
            surfaced: false,
            dismissed: false,
            file_path: event.file_path.clone(),
            project: event.project.clone(),
        })
    }

    /// Detect long debug cycles: same file saved >5 times in 10 minutes with
    /// interspersed CommandFail events.
    fn long_debug_cycle(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        if !matches!(event.event_type, EventType::FileSave) {
            return None;
        }

        let file_path = event.file_path.as_ref()?;
        let file_events = graph.get_events_for_file(file_path, 50).ok()?;

        let ten_min_ago = Utc::now() - Duration::minutes(10);

        let recent_saves: Vec<&CortexEvent> = file_events
            .iter()
            .filter(|e| {
                matches!(e.event_type, EventType::FileSave) && e.timestamp > ten_min_ago
            })
            .collect();

        if recent_saves.len() <= 5 {
            return None;
        }

        // Check for interspersed CommandFail events in the same timeframe
        let recent_summaries = graph.get_recent_events(100).ok()?;
        let fail_count = recent_summaries
            .iter()
            .filter(|e| e.event_type == "command_fail" && e.timestamp > ten_min_ago)
            .count();

        if fail_count == 0 {
            return None;
        }

        // Calculate duration from earliest recent save to now
        let earliest = recent_saves
            .last()
            .map(|e| e.timestamp)
            .unwrap_or_else(Utc::now);
        let duration_secs = (Utc::now() - earliest).num_seconds();

        let title = "Long debug cycle detected".to_string();
        let body = format!(
            "You've been iterating on {} for {} with failures. Consider stepping back to review the approach.",
            file_path,
            format_duration(duration_secs)
        );

        Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: event.id,
            insight_type: InsightType::Warning,
            title,
            body,
            relevance: 0.9,
            surfaced: false,
            dismissed: false,
            file_path: Some(file_path.clone()),
            project: event.project.clone(),
        })
    }
}

/// Extract the command prefix (first two words, e.g. "cargo build", "npm test").
fn extract_command_prefix(cmd: &str) -> String {
    let parts: Vec<&str> = cmd.split_whitespace().take(2).collect();
    parts.join(" ")
}

fn format_duration(seconds: i64) -> String {
    if seconds < 60 {
        format!("{} seconds", seconds)
    } else if seconds < 3600 {
        format!("{} minutes", seconds / 60)
    } else if seconds < 86400 {
        let hours = seconds / 3600;
        format!("{} hour{}", hours, if hours == 1 { "" } else { "s" })
    } else {
        let days = seconds / 86400;
        format!("{} day{}", days, if days == 1 { "" } else { "s" })
    }
}
