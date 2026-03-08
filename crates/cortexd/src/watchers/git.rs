use anyhow::Result;
use chrono::Utc;
use cortex_common::events::{CortexEvent, EventSource, EventType};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

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
        // Check if this dir itself is a git repo
        if dir.join(".git").exists() {
            repos.push(dir.clone());
        }
        // Check immediate subdirectories
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
    let oid = repo.head().ok().map(|r| r.target().unwrap_or_else(|| git2::Oid::zero()));
    let branch = repo
        .head()
        .ok()
        .and_then(|r| r.shorthand().map(|s| s.to_string()));
    (oid, branch)
}

pub async fn run(config: Arc<CortexConfig>, bus: EventBus) -> Result<()> {
    let repos = find_git_repos(&config.watch_dirs);
    if repos.is_empty() {
        tracing::info!("no git repos found to watch");
        return Ok(());
    }

    tracing::info!("git watcher monitoring {} repos", repos.len());

    let mut states: HashMap<PathBuf, RepoState> = HashMap::new();

    // Initialize states
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

            // Detect branch switch
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
                    tracing::debug!("git checkout detected: {:?} -> {}", state.head_branch, branch);
                }
                state.head_branch = current_branch.clone();
            }

            // Detect new commit
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
