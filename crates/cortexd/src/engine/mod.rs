pub mod claude;
pub mod ranker;
pub mod rules;
pub mod trigger;

use anyhow::Result;
use chrono::{Duration, Utc};
use std::sync::Arc;
use tokio::sync::broadcast;

use cortex_common::events::{CortexEvent, EventType};

use crate::graph::KnowledgeGraph;

use self::claude::{ClaudeClient, PromptType};
use self::ranker::InsightRanker;
use self::rules::LocalRules;
use self::trigger::TriggerEvaluator;

pub struct InferenceEngine {
    graph: Arc<KnowledgeGraph>,
    trigger: TriggerEvaluator,
    rules: LocalRules,
    ranker: InsightRanker,
    claude: Option<ClaudeClient>,
    session_id: String,
    session_start: chrono::DateTime<Utc>,
    session_events: Vec<CortexEvent>,
}

impl InferenceEngine {
    pub fn new(graph: Arc<KnowledgeGraph>, insight_threshold: f64) -> Self {
        Self {
            graph,
            trigger: TriggerEvaluator::new(),
            rules: LocalRules::new(),
            ranker: InsightRanker::new(insight_threshold),
            claude: None,
            session_id: generate_session_id(),
            session_start: Utc::now(),
            session_events: Vec::new(),
        }
    }

    /// Enable Claude-powered insight generation.
    pub fn with_claude(mut self, api_key: String, max_calls_per_hour: u32) -> Self {
        let client = ClaudeClient::new(api_key, max_calls_per_hour);
        if client.is_enabled() {
            self.claude = Some(client);
        }
        self
    }

    pub async fn run(mut self, mut rx: broadcast::Receiver<CortexEvent>) -> Result<()> {
        tracing::info!("inference engine started (session: {})", self.session_id);

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = self.process_event(&event).await {
                        tracing::error!("inference engine error processing event: {}", e);
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("inference engine lagged, skipped {} events", n);
                }
                Err(broadcast::error::RecvError::Closed) => {
                    tracing::info!("event bus closed, inference engine shutting down");
                    break;
                }
            }
        }

        Ok(())
    }

    async fn process_event(&mut self, event: &CortexEvent) -> Result<()> {
        // Session management: detect gaps >30min as new session
        self.maybe_rotate_session(event);

        // Buffer the event for session tracking
        self.session_events.push(event.clone());

        // Check if this event should trigger analysis
        if !self.trigger.should_trigger(event, &self.graph) {
            return Ok(());
        }

        tracing::debug!("event triggered analysis: {:?}", event.event_type);

        // Run local rules
        let insights = self.rules.evaluate(event, &self.graph);

        // Score and store qualifying insights
        for mut insight in insights {
            let score = self.ranker.score(&insight, 1);
            insight.relevance = score;

            if self.ranker.passes_threshold(score) {
                tracing::info!("storing insight: {} (score: {:.2})", insight.title, score);
                self.graph.store_insight(&insight)?;
            } else {
                tracing::debug!(
                    "insight below threshold: {} (score: {:.2})",
                    insight.title,
                    score
                );
            }
        }

        // Claude-powered insights for high-signal events
        self.maybe_call_claude(event).await?;

        Ok(())
    }

    /// Call Claude for high-signal events if enabled.
    async fn maybe_call_claude(&mut self, event: &CortexEvent) -> Result<()> {
        let claude = match self.claude.as_mut() {
            Some(c) if c.is_enabled() => c,
            _ => return Ok(()),
        };

        let (context, prompt_type) = match event.event_type {
            EventType::CommandFail => {
                // Only call Claude if >3 similar past failures
                let recent = self.graph.get_recent_events(100).ok();
                let fail_count = recent
                    .map(|events| {
                        let thirty_min_ago = Utc::now() - Duration::minutes(30);
                        events
                            .iter()
                            .filter(|e| {
                                e.event_type == "command_fail" && e.timestamp > thirty_min_ago
                            })
                            .count()
                    })
                    .unwrap_or(0);

                if fail_count <= 3 {
                    return Ok(());
                }

                let cmd = event
                    .payload
                    .get("cmd")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");

                let context = format!(
                    "Command '{}' has failed {} times in the last 30 minutes. \
                     Project: {}. Session has {} events so far.",
                    cmd,
                    fail_count,
                    event.project.as_deref().unwrap_or("unknown"),
                    self.session_events.len()
                );

                (context, PromptType::ErrorCorrelation)
            }
            EventType::GitCommit => {
                let msg = event
                    .payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("no message");

                let files_touched: Vec<String> = self
                    .session_events
                    .iter()
                    .filter_map(|e| e.file_path.clone())
                    .collect::<std::collections::HashSet<_>>()
                    .into_iter()
                    .take(10)
                    .collect();

                let context = format!(
                    "Git commit: '{}'. Project: {}. Files touched this session: {}.",
                    msg,
                    event.project.as_deref().unwrap_or("unknown"),
                    files_touched.join(", ")
                );

                (context, PromptType::FileContext)
            }
            _ => return Ok(()),
        };

        tracing::debug!("calling claude for {:?}", prompt_type);

        match claude.generate_insight(&context, prompt_type).await {
            Ok(Some(mut insight)) => {
                insight.trigger_event = event.id;
                insight.project = event.project.clone();

                let score = self.ranker.score(&insight, 1);
                insight.relevance = score;

                if self.ranker.passes_threshold(score) {
                    tracing::info!(
                        "storing claude insight: {} (score: {:.2})",
                        insight.title,
                        score
                    );
                    self.graph.store_insight(&insight)?;
                }
            }
            Ok(None) => {
                tracing::debug!("claude returned no insight (rate limited or empty)");
            }
            Err(e) => {
                tracing::warn!("claude API call failed: {}", e);
            }
        }

        Ok(())
    }

    /// Detect session gaps >30 minutes and rotate the session.
    fn maybe_rotate_session(&mut self, event: &CortexEvent) {
        let gap = (event.timestamp - self.session_start).num_seconds();

        // If more than 30 minutes since the last session activity, start a new session
        if let Some(last_event) = self.session_events.last() {
            let since_last = (event.timestamp - last_event.timestamp).num_seconds();
            if since_last > 1800 {
                tracing::info!(
                    "session gap detected ({}s), rotating session {} -> new",
                    since_last,
                    self.session_id
                );
                self.session_id = generate_session_id();
                self.session_start = event.timestamp;
                self.session_events.clear();
                return;
            }
        }

        // Also rotate if this is somehow a very old session with a gap
        if gap > 1800 && self.session_events.is_empty() {
            self.session_id = generate_session_id();
            self.session_start = event.timestamp;
        }
    }
}

/// Generate a session ID from timestamp + random component.
fn generate_session_id() -> String {
    let ts = Utc::now().timestamp_millis();
    let rand_part: u32 = (ts as u32).wrapping_mul(2654435761); // Knuth multiplicative hash
    format!("{:x}-{:08x}", ts, rand_part)
}
