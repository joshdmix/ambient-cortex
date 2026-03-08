use chrono::{Duration, Utc};
use cortex_common::events::{CortexEvent, EventType};
use cortex_common::models::{Insight, InsightType};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::graph::KnowledgeGraph;

/// Local (Tier 1) rules that generate insights without API calls.
pub struct LocalRules {
    /// Track project switches: Vec<(project, timestamp_epoch)>
    project_switches: Mutex<Vec<(String, i64)>>,
    /// Track (file_save → command_run) pairs: (file, command) → count
    action_pairs: Mutex<HashMap<(String, String), u64>>,
    /// Recent file saves: (file_path, timestamp_epoch)
    recent_saves: Mutex<Vec<(String, i64)>>,
}

impl LocalRules {
    pub fn new() -> Self {
        Self {
            project_switches: Mutex::new(Vec::new()),
            action_pairs: Mutex::new(HashMap::new()),
            recent_saves: Mutex::new(Vec::new()),
        }
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

        if let Some(insight) = self.stale_branch(event, graph) {
            insights.push(insight);
        }

        if let Some(insight) = self.cross_project_pattern(event) {
            insights.push(insight);
        }

        if let Some(insight) = self.predictive_action(event) {
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

    /// When checking out a branch, warn if it was last active >7 days ago.
    fn stale_branch(
        &self,
        event: &CortexEvent,
        graph: &Arc<KnowledgeGraph>,
    ) -> Option<Insight> {
        if !matches!(event.event_type, EventType::GitCheckout) {
            return None;
        }

        let branch = event
            .payload
            .get("branch")
            .and_then(|v| v.as_str())?;

        // Look through recent events for past activity on this branch
        let recent = graph.get_recent_events(500).ok()?;

        let seven_days_ago = Utc::now() - Duration::days(7);

        // Find the most recent event mentioning this branch (excluding the current checkout)
        let last_branch_event = recent
            .iter()
            .skip(1) // skip the current event
            .find(|e| e.summary.contains(branch));

        let last_activity = match last_branch_event {
            Some(evt) if evt.timestamp < seven_days_ago => evt,
            _ => return None,
        };

        let days_ago = (Utc::now() - last_activity.timestamp).num_days();

        let title = format!("Stale branch: {}", branch);
        let body = format!(
            "This branch was last active {} days ago. Previous activity: {}",
            days_ago, last_activity.summary
        );

        Some(Insight {
            id: None,
            created_at: Utc::now(),
            trigger_event: event.id,
            insight_type: InsightType::Reminder,
            title,
            body,
            relevance: 0.75,
            surfaced: false,
            dismissed: false,
            file_path: None,
            project: event.project.clone(),
        })
    }

    /// Detect cross-project patterns: editing similar file types across multiple projects.
    fn cross_project_pattern(
        &self,
        event: &CortexEvent,
    ) -> Option<Insight> {
        let project = event.project.as_ref()?;
        let now = event.timestamp.timestamp();

        let mut switches = self.project_switches.lock().unwrap();

        // Record this project visit
        if switches.last().map(|(p, _)| p != project).unwrap_or(true) {
            switches.push((project.clone(), now));
        }

        // Trim entries older than 24 hours
        let day_ago = now - 86400;
        switches.retain(|(_, ts)| *ts > day_ago);

        // Count unique projects visited today
        let mut project_counts: HashMap<&str, usize> = HashMap::new();
        for (p, _) in switches.iter() {
            *project_counts.entry(p.as_str()).or_insert(0) += 1;
        }

        if project_counts.len() < 3 {
            return None;
        }

        // Check if the same file type pattern appears across projects
        let file_path = event.file_path.as_ref()?;
        let ext = file_path.rsplit('.').next().unwrap_or("");
        if ext.is_empty() {
            return None;
        }

        // Check if we've already fired recently (simple dedup: only fire when crossing 3 projects)
        let unique_projects: Vec<&str> = project_counts.keys().copied().collect();
        if unique_projects.len() == 3 {
            let project_list = unique_projects.join(", ");
            let title = "Cross-project pattern detected".to_string();
            let body = format!(
                "You've been working across {} projects today: {}",
                unique_projects.len(),
                project_list
            );

            return Some(Insight {
                id: None,
                created_at: Utc::now(),
                trigger_event: event.id,
                insight_type: InsightType::Suggestion,
                title,
                body,
                relevance: 0.6,
                surfaced: false,
                dismissed: false,
                file_path: None,
                project: Some(project.clone()),
            });
        }

        None
    }

    /// Learn from patterns: if after editing file X, the user always runs command Y,
    /// suggest Y when file X is saved again.
    fn predictive_action(
        &self,
        event: &CortexEvent,
    ) -> Option<Insight> {
        let now = event.timestamp.timestamp();

        match event.event_type {
            EventType::FileSave => {
                // Record this save for later correlation
                let file_path = event.file_path.as_ref()?;
                let mut saves = self.recent_saves.lock().unwrap();
                saves.push((file_path.clone(), now));
                // Keep only last 5 minutes of saves
                saves.retain(|(_, ts)| now - ts < 300);
                drop(saves);

                // Check if we should suggest a predicted command for this file
                let pairs = self.action_pairs.lock().unwrap();
                let best = pairs
                    .iter()
                    .filter(|((f, _), count)| f == file_path && **count > 5)
                    .max_by_key(|(_, count)| *count);

                match best {
                    Some(((_, cmd), count)) => {
                        let title = "Predicted next action".to_string();
                        let body = format!(
                            "You usually run '{}' after editing {} ({} times observed)",
                            cmd, file_path, count
                        );

                        Some(Insight {
                            id: None,
                            created_at: Utc::now(),
                            trigger_event: event.id,
                            insight_type: InsightType::Suggestion,
                            title,
                            body,
                            relevance: 0.7,
                            surfaced: false,
                            dismissed: false,
                            file_path: Some(file_path.clone()),
                            project: event.project.clone(),
                        })
                    }
                    None => None,
                }
            }
            EventType::CommandRun => {
                // Check if there was a recent file save, and record the pair
                let cmd = event
                    .payload
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if cmd.is_empty() {
                    return None;
                }

                let cmd_prefix = extract_command_prefix(cmd);
                let saves = self.recent_saves.lock().unwrap();

                // Find files saved in the last 5 minutes
                let recent_files: Vec<String> = saves
                    .iter()
                    .filter(|(_, ts)| now - ts < 300)
                    .map(|(f, _)| f.clone())
                    .collect();
                drop(saves);

                let mut pairs = self.action_pairs.lock().unwrap();
                for file in &recent_files {
                    let key = (file.clone(), cmd_prefix.clone());
                    *pairs.entry(key).or_insert(0) += 1;
                }

                None
            }
            _ => None,
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::store::Store;
    use crate::graph::KnowledgeGraph;
    use chrono::{Duration, Utc};
    use cortex_common::events::{CortexEvent, EventSource, EventType};
    use std::sync::Arc;
    use tempfile::NamedTempFile;

    fn make_graph() -> Arc<KnowledgeGraph> {
        let tmp = NamedTempFile::new().unwrap();
        let store = Store::new(tmp.path()).unwrap();
        // Keep the tempfile alive by leaking it so the DB isn't deleted
        let _ = tmp.into_temp_path();
        Arc::new(KnowledgeGraph::new(store))
    }

    fn make_event(
        event_type: EventType,
        file_path: Option<&str>,
        project: Option<&str>,
    ) -> CortexEvent {
        CortexEvent {
            id: None,
            timestamp: Utc::now(),
            event_type,
            source: EventSource::Filesystem,
            project: project.map(|s| s.to_string()),
            file_path: file_path.map(|s| s.to_string()),
            payload: serde_json::json!({}),
            session_id: None,
        }
    }

    fn make_event_at(
        event_type: EventType,
        file_path: Option<&str>,
        project: Option<&str>,
        timestamp: chrono::DateTime<Utc>,
    ) -> CortexEvent {
        let mut e = make_event(event_type, file_path, project);
        e.timestamp = timestamp;
        e
    }

    #[test]
    fn co_edit_reminder() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Build a strong co-edit relation by alternating saves of two files.
        // Each ingest_event calls update_file_relations which tracks co-edits.
        // We need the relation strength > 3.
        for i in 0..5 {
            let ts = Utc::now() - Duration::seconds(50 - i * 10);
            let e_a = make_event_at(EventType::FileSave, Some("/src/foo.rs"), Some("proj"), ts);
            graph.ingest_event(&e_a).unwrap();
            let e_b = make_event_at(
                EventType::FileSave,
                Some("/src/bar.rs"),
                Some("proj"),
                ts + Duration::seconds(1),
            );
            graph.ingest_event(&e_b).unwrap();
        }

        let event = make_event(EventType::FileSave, Some("/src/foo.rs"), Some("proj"));
        let insights = rules.evaluate(&event, &graph);

        assert!(
            insights.iter().any(|i| i.body.contains("/src/bar.rs")),
            "Expected co-edit insight mentioning bar.rs, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn co_edit_reminder_no_trigger() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Only one co-edit pair => strength = 1 (weak)
        let e_a = make_event(EventType::FileSave, Some("/src/foo.rs"), Some("proj"));
        graph.ingest_event(&e_a).unwrap();
        let e_b = make_event(EventType::FileSave, Some("/src/bar.rs"), Some("proj"));
        graph.ingest_event(&e_b).unwrap();

        let event = make_event(EventType::FileSave, Some("/src/foo.rs"), Some("proj"));
        let insights = rules.evaluate(&event, &graph);

        assert!(
            !insights
                .iter()
                .any(|i| i.title.contains("usually edit these files")),
            "Should not trigger co-edit reminder with weak relation"
        );
    }

    #[test]
    fn context_switch() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Insert an old event for the file (>1 hour ago)
        let old_time = Utc::now() - Duration::hours(2);
        let old_event = make_event_at(
            EventType::FileSave,
            Some("/src/main.rs"),
            Some("myproject"),
            old_time,
        );
        graph.ingest_event(&old_event).unwrap();

        // Insert enough events to pass the len >= 5 check on get_recent_events(20)
        for i in 0..5 {
            let filler = make_event_at(
                EventType::FileSave,
                Some(&format!("/src/filler{}.rs", i)),
                Some("myproject"),
                old_time + Duration::seconds(i as i64),
            );
            graph.ingest_event(&filler).unwrap();
        }

        // Now send a new event for the same file with current timestamp (>1 hour gap)
        let new_event = make_event(EventType::FileSave, Some("/src/main.rs"), Some("myproject"));
        graph.ingest_event(&new_event).unwrap();

        let insights = rules.evaluate(&new_event, &graph);

        assert!(
            insights.iter().any(|i| i.title.contains("Welcome back")),
            "Expected context switch insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn edit_revert_detector() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Insert >3 FileSave events for same file within 5 minutes
        for i in 0..4 {
            let evt = make_event_at(
                EventType::FileSave,
                Some("/src/buggy.rs"),
                Some("proj"),
                Utc::now() - Duration::seconds(60 * (3 - i)),
            );
            graph.ingest_event(&evt).unwrap();
        }

        // The 4th (current) event triggers the check
        let trigger = make_event(EventType::FileSave, Some("/src/buggy.rs"), Some("proj"));
        graph.ingest_event(&trigger).unwrap();

        let insights = rules.evaluate(&trigger, &graph);

        assert!(
            insights.iter().any(|i| i.title.contains("Rapid file saves")),
            "Expected edit-revert insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn edit_revert_no_trigger() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Only 2 saves -- should not trigger (threshold is >3)
        for i in 0..2 {
            let evt = make_event_at(
                EventType::FileSave,
                Some("/src/ok.rs"),
                Some("proj"),
                Utc::now() - Duration::seconds(30 * (1 - i)),
            );
            graph.ingest_event(&evt).unwrap();
        }

        let trigger = make_event(EventType::FileSave, Some("/src/ok.rs"), Some("proj"));
        graph.ingest_event(&trigger).unwrap();

        let insights = rules.evaluate(&trigger, &graph);

        assert!(
            !insights.iter().any(|i| i.title.contains("Rapid file saves")),
            "Should not trigger edit-revert with only 2 saves"
        );
    }

    #[test]
    fn error_pattern() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Insert >2 CommandFail events with same command prefix in 30 min
        for i in 0..3 {
            let mut evt = make_event_at(
                EventType::CommandFail,
                None,
                Some("proj"),
                Utc::now() - Duration::seconds(60 * (2 - i)),
            );
            evt.payload = serde_json::json!({"cmd": "cargo build --release"});
            graph.ingest_event(&evt).unwrap();
        }

        // Trigger event
        let mut trigger = make_event(EventType::CommandFail, None, Some("proj"));
        trigger.payload = serde_json::json!({"cmd": "cargo build --release"});
        graph.ingest_event(&trigger).unwrap();

        let insights = rules.evaluate(&trigger, &graph);

        assert!(
            insights
                .iter()
                .any(|i| i.title.contains("Repeated command failures")),
            "Expected error pattern insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn stale_branch() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Insert an old GitCheckout event for branch "feature-old" (>7 days ago)
        let old_time = Utc::now() - Duration::days(10);
        let mut old_event = make_event_at(
            EventType::GitCheckout,
            None,
            Some("proj"),
            old_time,
        );
        old_event.payload = serde_json::json!({"branch": "feature-old"});
        graph.ingest_event(&old_event).unwrap();

        // Now checkout that same branch again
        let mut trigger = make_event(EventType::GitCheckout, None, Some("proj"));
        trigger.payload = serde_json::json!({"branch": "feature-old"});
        graph.ingest_event(&trigger).unwrap();

        let insights = rules.evaluate(&trigger, &graph);

        assert!(
            insights
                .iter()
                .any(|i| i.title.contains("Stale branch")),
            "Expected stale branch insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn cross_project_pattern() {
        let rules = LocalRules::new();
        let graph = make_graph();

        // Send events from 3 different projects
        let e1 = make_event(EventType::FileSave, Some("/a/foo.rs"), Some("project-a"));
        let e2 = make_event(EventType::FileSave, Some("/b/bar.rs"), Some("project-b"));
        let e3 = make_event(EventType::FileSave, Some("/c/baz.rs"), Some("project-c"));

        // Evaluate each -- insight should fire on the 3rd
        let _ = rules.evaluate(&e1, &graph);
        let _ = rules.evaluate(&e2, &graph);
        let insights = rules.evaluate(&e3, &graph);

        assert!(
            insights
                .iter()
                .any(|i| i.title.contains("Cross-project pattern")),
            "Expected cross-project insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn predictive_action() {
        let rules = LocalRules::new();
        let graph = make_graph();

        // Build up >5 (file_save -> command_run) pairs
        for _ in 0..6 {
            let save = make_event(EventType::FileSave, Some("/src/lib.rs"), Some("proj"));
            let _ = rules.evaluate(&save, &graph);

            let mut run = make_event(EventType::CommandRun, Some("/src/lib.rs"), Some("proj"));
            run.payload = serde_json::json!({"cmd": "cargo test"});
            let _ = rules.evaluate(&run, &graph);
        }

        // Verify the pair was tracked in action_pairs
        let pairs = rules.action_pairs.lock().unwrap();
        let key = ("/src/lib.rs".to_string(), "cargo test".to_string());
        let count = pairs.get(&key).copied().unwrap_or(0);
        assert!(
            count >= 6,
            "Expected action pair count >= 6, got {}",
            count
        );
        drop(pairs);

        // Now a new FileSave should generate a predictive insight
        let save = make_event(EventType::FileSave, Some("/src/lib.rs"), Some("proj"));
        let insights = rules.evaluate(&save, &graph);
        assert!(
            insights
                .iter()
                .any(|i| i.title.contains("Predicted next action")),
            "Expected predictive action insight after >5 pairs, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn long_debug_cycle() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // Insert >5 FileSave events for same file within 10 minutes
        for i in 0..6 {
            let evt = make_event_at(
                EventType::FileSave,
                Some("/src/tricky.rs"),
                Some("proj"),
                Utc::now() - Duration::seconds(60 * (5 - i)),
            );
            graph.ingest_event(&evt).unwrap();
        }

        // Insert interspersed CommandFail events
        for i in 0..2 {
            let mut evt = make_event_at(
                EventType::CommandFail,
                None,
                Some("proj"),
                Utc::now() - Duration::seconds(60 * (4 - i)),
            );
            evt.payload = serde_json::json!({"cmd": "cargo test", "exit_code": 1});
            graph.ingest_event(&evt).unwrap();
        }

        // Trigger with another FileSave
        let trigger = make_event(EventType::FileSave, Some("/src/tricky.rs"), Some("proj"));
        graph.ingest_event(&trigger).unwrap();

        let insights = rules.evaluate(&trigger, &graph);
        assert!(
            insights
                .iter()
                .any(|i| i.title.contains("Long debug cycle")),
            "Expected long debug cycle insight, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }

    #[test]
    fn evaluate_returns_empty() {
        let graph = make_graph();
        let rules = LocalRules::new();

        // A benign FileOpen event with no history should produce nothing
        let event = make_event(EventType::FileOpen, Some("/tmp/nothing.txt"), None);
        let insights = rules.evaluate(&event, &graph);

        assert!(
            insights.is_empty(),
            "Expected empty insights for benign event, got: {:?}",
            insights.iter().map(|i| &i.title).collect::<Vec<_>>()
        );
    }
}
