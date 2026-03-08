use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventSource, EventType};
use tokio::io::AsyncBufReadExt;

use crate::bus::EventBus;
use crate::config::CortexConfig;

pub async fn run(bus: EventBus) -> Result<()> {
    let pipe_path = CortexConfig::pipe_path("editor.pipe");

    if let Some(parent) = pipe_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    #[cfg(unix)]
    if !pipe_path.exists() {
        use std::process::Command;
        let _ = Command::new("mkfifo").arg(&pipe_path).status();
    }

    tracing::info!("editor watcher listening on {}", pipe_path.display());

    loop {
        let file = match tokio::fs::File::open(&pipe_path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("failed to open editor pipe: {}, retrying in 1s", e);
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
                    let event_type_str = payload
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");

                    let event_type = match event_type_str {
                        "file_open" => EventType::FileOpen,
                        "file_save" => EventType::FileSave,
                        "file_delete" => EventType::FileDelete,
                        _ => continue,
                    };

                    let file_path = payload
                        .get("path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let event = CortexEvent {
                        id: None,
                        timestamp: Utc::now(),
                        event_type,
                        source: EventSource::Editor,
                        project: None,
                        file_path,
                        payload: payload.clone(),
                        session_id: None,
                    };

                    bus.publish(event);
                    tracing::debug!("editor event: {}", event_type_str);
                }
                Err(e) => {
                    tracing::warn!("failed to parse editor event: {}", e);
                }
            }
        }

        tracing::debug!("editor pipe writer disconnected, reopening");
    }
}
