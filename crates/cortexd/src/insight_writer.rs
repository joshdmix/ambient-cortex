use std::sync::Arc;

use crate::config::CortexConfig;
use crate::graph::KnowledgeGraph;

/// Periodically writes the top pending insight to current_insight.json
/// for consumption by shell prompts and tmux status bars.
pub async fn run(graph: Arc<KnowledgeGraph>) {
    let insight_path = CortexConfig::data_dir().join("current_insight.json");
    let interval = std::time::Duration::from_secs(5);

    loop {
        tokio::time::sleep(interval).await;

        let json = match graph.get_pending_insights() {
            Ok(insights) if !insights.is_empty() => {
                let top = &insights[0];
                let insight_type = match top.insight_type {
                    cortex_common::models::InsightType::Warning => "warning",
                    cortex_common::models::InsightType::Reminder => "reminder",
                    cortex_common::models::InsightType::Suggestion => "suggestion",
                    cortex_common::models::InsightType::History => "history",
                };
                serde_json::json!({
                    "title": top.title,
                    "body": top.body,
                    "type": insight_type,
                })
                .to_string()
            }
            _ => {
                // No insights — remove the file so prompt shows nothing
                let _ = std::fs::remove_file(&insight_path);
                continue;
            }
        };

        if let Err(e) = std::fs::write(&insight_path, &json) {
            tracing::warn!("failed to write current_insight.json: {}", e);
        }
    }
}
