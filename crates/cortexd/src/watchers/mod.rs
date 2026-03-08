pub mod editor;
pub mod filesystem;
pub mod git;
pub mod terminal;

use anyhow::Result;
use std::sync::Arc;
use tokio::task::JoinHandle;

use crate::bus::EventBus;
use crate::config::CortexConfig;

pub struct WatcherManager {
    handles: Vec<(&'static str, JoinHandle<()>)>,
}

impl WatcherManager {
    pub fn new() -> Self {
        Self {
            handles: Vec::new(),
        }
    }

    pub fn start_all(&mut self, config: Arc<CortexConfig>, bus: EventBus) -> Result<()> {
        // Filesystem watcher
        let fs_bus = bus.clone();
        let fs_config = config.clone();
        let fs_handle = tokio::spawn(async move {
            if let Err(e) = filesystem::run(fs_config, fs_bus).await {
                tracing::error!("filesystem watcher error: {}", e);
            }
        });
        self.handles.push(("filesystem", fs_handle));

        // Terminal watcher
        let term_bus = bus.clone();
        let term_handle = tokio::spawn(async move {
            if let Err(e) = terminal::run(term_bus).await {
                tracing::error!("terminal watcher error: {}", e);
            }
        });
        self.handles.push(("terminal", term_handle));

        // Git watcher
        let git_bus = bus.clone();
        let git_config = config.clone();
        let git_handle = tokio::spawn(async move {
            if let Err(e) = git::run(git_config, git_bus).await {
                tracing::error!("git watcher error: {}", e);
            }
        });
        self.handles.push(("git", git_handle));

        // Editor watcher (stub)
        editor::run();

        tracing::info!("all watchers started");
        Ok(())
    }

    pub fn active_watchers(&self) -> Vec<String> {
        self.handles
            .iter()
            .filter(|(_, h)| !h.is_finished())
            .map(|(name, _)| name.to_string())
            .collect()
    }
}
