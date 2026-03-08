use anyhow::{bail, Result};
use cortex_common::protocol::{Request, Response};

use super::send_request;

pub async fn run() -> Result<()> {
    let response = send_request(Request::Status).await?;

    match response {
        Response::Status(status) => {
            let hours = status.uptime_secs / 3600;
            let minutes = (status.uptime_secs % 3600) / 60;
            let seconds = status.uptime_secs % 60;

            println!("Cortex Daemon Status");
            println!("{}", "-".repeat(40));
            println!(
                "  Uptime:           {}h {}m {}s",
                hours, minutes, seconds
            );
            println!("  Events recorded:  {}", status.event_count);
            println!("  Insights:         {}", status.insight_count);
            println!(
                "  Active watchers:  {}",
                if status.watchers_active.is_empty() {
                    "none".to_string()
                } else {
                    status.watchers_active.join(", ")
                }
            );
        }
        Response::Error(msg) => bail!("daemon error: {}", msg),
        _ => bail!("unexpected response from daemon"),
    }

    Ok(())
}
