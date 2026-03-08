pub mod claude;
pub mod ranker;
pub mod rules;
pub mod trigger;

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::broadcast;

use cortex_common::events::CortexEvent;

use crate::graph::KnowledgeGraph;

use self::ranker::InsightRanker;
use self::rules::LocalRules;
use self::trigger::TriggerEvaluator;

pub struct InferenceEngine {
    graph: Arc<KnowledgeGraph>,
    trigger: TriggerEvaluator,
    rules: LocalRules,
    ranker: InsightRanker,
}

impl InferenceEngine {
    pub fn new(graph: Arc<KnowledgeGraph>, insight_threshold: f64) -> Self {
        Self {
            graph,
            trigger: TriggerEvaluator::new(),
            rules: LocalRules::new(),
            ranker: InsightRanker::new(insight_threshold),
        }
    }

    pub async fn run(self, mut rx: broadcast::Receiver<CortexEvent>) -> Result<()> {
        tracing::info!("inference engine started");

        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Err(e) = self.process_event(&event) {
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

    fn process_event(&self, event: &CortexEvent) -> Result<()> {
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

        Ok(())
    }
}
