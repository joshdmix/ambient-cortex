use anyhow::{bail, Result};
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run(limit: usize) -> Result<()> {
    let response = send_request(Request::History { limit }).await?;

    match response {
        Response::HistoryResult(events) => {
            if events.is_empty() {
                println!("No events recorded yet.");
                return Ok(());
            }

            println!(
                "{:<20} {:<14} {:<12} {}",
                "Timestamp", "Type", "Source", "Summary"
            );
            println!("{}", "-".repeat(70));

            for event in &events {
                println!(
                    "{:<20} {:<14} {:<12} {}",
                    event.timestamp.format("%Y-%m-%d %H:%M"),
                    event.event_type,
                    event.source,
                    event.summary,
                );
            }

            println!("\n{} event(s) shown.", events.len());
        }
        Response::Error(msg) => bail!("daemon error: {}", msg),
        _ => bail!("unexpected response from daemon"),
    }

    Ok(())
}
