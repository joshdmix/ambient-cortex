use std::sync::Arc;
use std::time::Duration;

use crate::graph::KnowledgeGraph;

pub async fn run(graph: Arc<KnowledgeGraph>, notifications_enabled: bool) {
    if !notifications_enabled {
        return;
    }

    tracing::info!("notifier started, polling every 10s");

    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;

        let insights = match graph.get_pending_insights() {
            Ok(insights) => insights,
            Err(e) => {
                tracing::error!("notifier: failed to get pending insights: {}", e);
                continue;
            }
        };

        for insight in insights {
            if insight.relevance <= 0.9 {
                continue;
            }

            let id = match insight.id {
                Some(id) => id,
                None => continue,
            };

            // Send macOS notification via osascript
            let body = insight.body.replace('\\', "\\\\").replace('"', "\\\"");
            let title = insight.title.replace('\\', "\\\\").replace('"', "\\\"");
            let script = format!(
                "display notification \"{}\" with title \"{}\"",
                body, title
            );

            match tokio::process::Command::new("osascript")
                .arg("-e")
                .arg(&script)
                .output()
                .await
            {
                Ok(output) => {
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        tracing::warn!("notifier: osascript failed: {}", stderr);
                    }
                }
                Err(e) => {
                    tracing::warn!("notifier: failed to run osascript: {}", e);
                }
            }

            if let Err(e) = graph.mark_insight_surfaced(id) {
                tracing::error!("notifier: failed to mark insight {} surfaced: {}", id, e);
            }
        }
    }
}
