use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventSource, EventType};
use tokio::io::AsyncBufReadExt;

use crate::bus::EventBus;
use crate::config::CortexConfig;

pub async fn run(bus: EventBus) -> Result<()> {
    let pipe_path = CortexConfig::pipe_path("terminal.pipe");

    // Create the parent directory if needed
    if let Some(parent) = pipe_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Create the named pipe if it doesn't exist
    #[cfg(unix)]
    if !pipe_path.exists() {
        use std::process::Command;
        let _ = Command::new("mkfifo")
            .arg(&pipe_path)
            .status();
    }

    tracing::info!("terminal watcher listening on {}", pipe_path.display());

    // Continuously read from the pipe
    loop {
        // Open the pipe for reading (this blocks until a writer connects)
        let file = match tokio::fs::File::open(&pipe_path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("failed to open terminal pipe: {}, retrying in 1s", e);
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
        };

        let reader = tokio::io::BufReader::new(file);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            if line.trim().is_empty() {
                continue;
            }

            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(payload) => {
                    let exit_code = payload.get("exit").and_then(|v| v.as_i64()).unwrap_or(0);
                    let event_type = if exit_code == 0 {
                        EventType::CommandRun
                    } else {
                        EventType::CommandFail
                    };

                    let cwd = payload
                        .get("cwd")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let event = CortexEvent {
                        id: None,
                        timestamp: Utc::now(),
                        event_type,
                        source: EventSource::Terminal,
                        project: cwd,
                        file_path: None,
                        payload: payload.clone(),
                        session_id: None,
                    };

                    bus.publish(event);
                    tracing::debug!("terminal event published");
                }
                Err(e) => {
                    tracing::warn!("failed to parse terminal event: {}", e);
                }
            }
        }

        // Writer disconnected, loop back to reopen
        tracing::debug!("terminal pipe writer disconnected, reopening");
    }
}
