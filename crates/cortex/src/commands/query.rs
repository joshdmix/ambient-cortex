use anyhow::{bail, Result};
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run(file_path: &str) -> Result<()> {
    let response = send_request(Request::Query {
        file_path: file_path.to_string(),
    })
    .await?;

    match response {
        Response::QueryResult(info) => {
            println!("File: {}", info.path);
            println!("{}", "-".repeat(60));
            println!("  Touch count:    {}", info.touch_count);
            println!("  Total time:     {}s", info.total_time_s);
            println!(
                "  Last touched:   {}",
                info.last_touched.format("%Y-%m-%d %H:%M:%S UTC")
            );

            if !info.related_files.is_empty() {
                println!("\n  Related files:");
                for f in &info.related_files {
                    println!("    - {}", f);
                }
            }

            if !info.recent_events.is_empty() {
                println!("\n  Recent events:");
                println!(
                    "    {:<20} {:<14} {:<12} Summary",
                    "Timestamp", "Type", "Source"
                );
                for event in &info.recent_events {
                    println!(
                        "    {:<20} {:<14} {:<12} {}",
                        event.timestamp.format("%Y-%m-%d %H:%M"),
                        event.event_type,
                        event.source,
                        event.summary,
                    );
                }
            }

            if !info.insights.is_empty() {
                println!("\n  Insights:");
                for insight in &info.insights {
                    println!(
                        "    [{}] {} (relevance: {:.2})",
                        insight.insight_type, insight.title, insight.relevance
                    );
                    println!("      {}", insight.body);
                }
            }
        }
        Response::Error(msg) => bail!("daemon error: {}", msg),
        _ => bail!("unexpected response from daemon"),
    }

    Ok(())
}
