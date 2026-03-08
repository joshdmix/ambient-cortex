use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventSource, EventType};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::sync::Arc;
use tokio::sync::mpsc;

use crate::bus::EventBus;
use crate::config::CortexConfig;

const BUILT_IN_EXCLUDES: &[&str] = &[
    "target/",
    "node_modules/",
    ".git/objects/",
    ".git/refs/",
    ".git/logs/",
];

fn should_exclude(path: &std::path::Path, config: &CortexConfig) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in BUILT_IN_EXCLUDES {
        if path_str.contains(pattern) {
            return true;
        }
    }

    for pattern in &config.exclude_patterns {
        if path_str.contains(pattern.as_str()) {
            return true;
        }
    }

    false
}

fn detect_project_root(path: &std::path::Path) -> Option<String> {
    let mut current = path.parent()?;
    loop {
        if current.join(".git").exists() || current.join("Cargo.toml").exists() {
            return Some(current.to_string_lossy().to_string());
        }
        current = current.parent()?;
    }
}

pub async fn run(config: Arc<CortexConfig>, bus: EventBus) -> Result<()> {
    let (tx, mut rx) = mpsc::channel::<notify::Event>(256);
    let debounce_ms = config.debounce_ms;

    let mut watcher: RecommendedWatcher = notify::recommended_watcher(move |res| match res {
        Ok(event) => {
            let _ = tx.blocking_send(event);
        }
        Err(e) => {
            tracing::error!("filesystem watcher error: {}", e);
        }
    })?;

    for dir in &config.watch_dirs {
        if dir.exists() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
            tracing::info!("watching directory: {}", dir.display());
        } else {
            tracing::warn!("watch directory does not exist: {}", dir.display());
        }
    }

    // Use a simple time-based deduplication approach
    let mut last_events: std::collections::HashMap<String, std::time::Instant> =
        std::collections::HashMap::new();
    let debounce_duration = std::time::Duration::from_millis(debounce_ms);

    tracing::info!("filesystem watcher started");

    // Keep watcher alive
    let _watcher = watcher;

    while let Some(event) = rx.recv().await {
        match event.kind {
            EventKind::Modify(_) | EventKind::Create(_) => {}
            _ => continue,
        }

        for path in &event.paths {
            if should_exclude(path, &config) {
                continue;
            }

            // Only watch files, not directories
            if path.is_dir() {
                continue;
            }

            let path_str = path.to_string_lossy().to_string();

            // Debounce: skip if we saw this path recently
            let now = std::time::Instant::now();
            if let Some(last) = last_events.get(&path_str) {
                if now.duration_since(*last) < debounce_duration {
                    continue;
                }
            }
            last_events.insert(path_str.clone(), now);

            let project = detect_project_root(path);

            let cortex_event = CortexEvent {
                id: None,
                timestamp: Utc::now(),
                event_type: EventType::FileSave,
                source: EventSource::Filesystem,
                project,
                file_path: Some(path_str),
                payload: serde_json::json!({
                    "kind": format!("{:?}", event.kind),
                }),
                session_id: None,
            };

            bus.publish(cortex_event);
            tracing::debug!("filesystem event published for {}", path.display());
        }
    }

    Ok(())
}
