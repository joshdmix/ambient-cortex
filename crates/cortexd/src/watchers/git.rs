use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventSource, EventType};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;

use crate::bus::EventBus;
use crate::config::CortexConfig;

const POLL_INTERVAL_SECS: u64 = 5;

struct RepoState {
    head_oid: Option<git2::Oid>,
    head_branch: Option<String>,
}

fn find_git_repos(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    for dir in dirs {
        if !dir.exists() {
            continue;
        }
        if dir.join(".git").exists() {
            repos.push(dir.clone());
        }
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() && path.join(".git").exists() {
                    repos.push(path);
                }
            }
        }
    }
    repos
}

fn get_head_info(repo: &git2::Repository) -> (Option<git2::Oid>, Option<String>) {
    let oid = repo
        .head()
        .ok()
        .map(|r| r.target().unwrap_or_else(git2::Oid::zero));
    let branch = repo
        .head()
        .ok()
        .and_then(|r| r.shorthand().map(|s| s.to_string()));
    (oid, branch)
}

/// Dual-mode git watcher: named pipe for hook-based events + polling fallback.
pub async fn run(config: Arc<CortexConfig>, bus: EventBus) -> Result<()> {
    let pipe_bus = bus.clone();
    let poll_bus = bus;
    let poll_config = config;

    // Spawn pipe listener for hook-based events
    tokio::spawn(async move {
        if let Err(e) = run_pipe_listener(pipe_bus).await {
            tracing::error!("git pipe listener error: {}", e);
        }
    });

    // Run polling fallback
    run_polling(poll_config, poll_bus).await
}

/// Listen on named pipe for git hook events (post-commit, post-checkout).
async fn run_pipe_listener(bus: EventBus) -> Result<()> {
    let pipe_path = CortexConfig::pipe_path("git.pipe");

    if let Some(parent) = pipe_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    #[cfg(unix)]
    if !pipe_path.exists() {
        use std::process::Command;
        let _ = Command::new("mkfifo").arg(&pipe_path).status();
    }

    tracing::info!("git pipe listener on {}", pipe_path.display());

    loop {
        let file = match tokio::fs::File::open(&pipe_path).await {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!("failed to open git pipe: {}, retrying in 1s", e);
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
                        "git_commit" => EventType::GitCommit,
                        "git_checkout" => EventType::GitCheckout,
                        _ => continue,
                    };

                    let branch = payload
                        .get("branch")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    let event = CortexEvent {
                        id: None,
                        timestamp: Utc::now(),
                        event_type,
                        source: EventSource::Git,
                        project: branch.clone(),
                        file_path: None,
                        payload: payload.clone(),
                        session_id: None,
                    };

                    bus.publish(event);
                    tracing::debug!("git pipe event: {}", event_type_str);
                }
                Err(e) => {
                    tracing::warn!("failed to parse git pipe event: {}", e);
                }
            }
        }

        tracing::debug!("git pipe writer disconnected, reopening");
    }
}

/// Polling fallback: check git repos every 5 seconds for changes.
async fn run_polling(config: Arc<CortexConfig>, bus: EventBus) -> Result<()> {
    let repos = find_git_repos(&config.watch_dirs);
    if repos.is_empty() {
        tracing::info!("no git repos found to watch");
        return Ok(());
    }

    tracing::info!("git watcher monitoring {} repos (polling)", repos.len());

    let mut states: HashMap<PathBuf, RepoState> = HashMap::new();

    for repo_path in &repos {
        if let Ok(repo) = git2::Repository::open(repo_path) {
            let (oid, branch) = get_head_info(&repo);
            states.insert(
                repo_path.clone(),
                RepoState {
                    head_oid: oid,
                    head_branch: branch,
                },
            );
        }
    }

    let interval = std::time::Duration::from_secs(POLL_INTERVAL_SECS);

    loop {
        tokio::time::sleep(interval).await;

        for repo_path in &repos {
            let repo = match git2::Repository::open(repo_path) {
                Ok(r) => r,
                Err(_) => continue,
            };

            let (current_oid, current_branch) = get_head_info(&repo);

            let state = states.entry(repo_path.clone()).or_insert(RepoState {
                head_oid: None,
                head_branch: None,
            });

            if state.head_branch != current_branch {
                if let Some(ref branch) = current_branch {
                    let event = CortexEvent {
                        id: None,
                        timestamp: Utc::now(),
                        event_type: EventType::GitCheckout,
                        source: EventSource::Git,
                        project: Some(repo_path.to_string_lossy().to_string()),
                        file_path: None,
                        payload: serde_json::json!({
                            "branch": branch,
                            "previous_branch": state.head_branch,
                        }),
                        session_id: None,
                    };
                    bus.publish(event);
                    tracing::debug!(
                        "git checkout detected: {:?} -> {}",
                        state.head_branch,
                        branch
                    );
                }
                state.head_branch = current_branch.clone();
            }

            if state.head_oid != current_oid {
                if let Some(oid) = current_oid {
                    if let Ok(commit) = repo.find_commit(oid) {
                        let message = commit.message().unwrap_or("").to_string();
                        let author = commit.author().name().unwrap_or("unknown").to_string();

                        let event = CortexEvent {
                            id: None,
                            timestamp: Utc::now(),
                            event_type: EventType::GitCommit,
                            source: EventSource::Git,
                            project: Some(repo_path.to_string_lossy().to_string()),
                            file_path: None,
                            payload: serde_json::json!({
                                "commit": oid.to_string(),
                                "message": message.trim(),
                                "author": author,
                                "branch": current_branch,
                            }),
                            session_id: None,
                        };
                        bus.publish(event);
                        tracing::debug!("git commit detected: {}", &oid.to_string()[..8]);
                    }
                }
                state.head_oid = current_oid;
            }
        }
    }
}
