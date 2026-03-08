use anyhow::Result;
use cortex_common::protocol::{DaemonStatus, InsightSummary, Request, Response};
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use crate::config::CortexConfig;
use crate::graph::KnowledgeGraph;
use crate::watchers::WatcherManager;

pub async fn run(
    graph: Arc<KnowledgeGraph>,
    watcher_manager: Arc<std::sync::Mutex<WatcherManager>>,
    start_time: Instant,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<()> {
    let socket_path = CortexConfig::socket_path();

    // Remove stale socket file
    if socket_path.exists() {
        tokio::fs::remove_file(&socket_path).await?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    tracing::info!("unix socket server listening at {}", socket_path.display());

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                match accept_result {
                    Ok((stream, _addr)) => {
                        let graph = graph.clone();
                        let watcher_manager = watcher_manager.clone();
                        let start = start_time;
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, graph, watcher_manager, start).await {
                                tracing::error!("connection handler error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        tracing::error!("accept error: {}", e);
                    }
                }
            }
            _ = shutdown_rx.changed() => {
                tracing::info!("server received shutdown signal");
                break;
            }
        }
    }

    // Clean up socket file
    let _ = tokio::fs::remove_file(&socket_path).await;
    Ok(())
}

async fn handle_connection(
    stream: tokio::net::UnixStream,
    graph: Arc<KnowledgeGraph>,
    watcher_manager: Arc<std::sync::Mutex<WatcherManager>>,
    start_time: Instant,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    while reader.read_line(&mut line).await? > 0 {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            line.clear();
            continue;
        }

        let response = match serde_json::from_str::<Request>(trimmed) {
            Ok(request) => handle_request(request, &graph, &watcher_manager, start_time),
            Err(e) => Response::Error(format!("invalid request: {}", e)),
        };

        let response_json = serde_json::to_string(&response)?;
        writer.write_all(response_json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        line.clear();
    }

    Ok(())
}

fn handle_request(
    request: Request,
    graph: &Arc<KnowledgeGraph>,
    watcher_manager: &Arc<std::sync::Mutex<WatcherManager>>,
    start_time: Instant,
) -> Response {
    match request {
        Request::Status => {
            let uptime = start_time.elapsed().as_secs();
            let (event_count, insight_count) = graph.get_stats().unwrap_or((0, 0));
            let watchers_active = watcher_manager
                .lock()
                .map(|wm| wm.active_watchers())
                .unwrap_or_default();

            Response::Status(DaemonStatus {
                uptime_secs: uptime,
                event_count: event_count as u64,
                insight_count: insight_count as u64,
                watchers_active,
            })
        }
        Request::Query { file_path } => match graph.query_file(&file_path) {
            Ok(info) => Response::QueryResult(info),
            Err(e) => Response::Error(format!("query error: {}", e)),
        },
        Request::History { limit } => match graph.get_recent_events(limit) {
            Ok(events) => Response::HistoryResult(events),
            Err(e) => Response::Error(format!("history error: {}", e)),
        },
        Request::Search { query } => match graph.semantic_search(&query, 20) {
            Ok(hits) => Response::SearchResult(hits),
            Err(e) => Response::Error(format!("search error: {}", e)),
        },
        Request::DismissInsight { insight_id } => match graph.dismiss_insight(insight_id) {
            Ok(()) => Response::Ok,
            Err(e) => Response::Error(format!("dismiss error: {}", e)),
        },
        Request::UpvoteInsight { insight_id } => match graph.upvote_insight(insight_id) {
            Ok(()) => Response::Ok,
            Err(e) => Response::Error(format!("upvote error: {}", e)),
        },
        Request::GetInsights => match graph.get_pending_insights() {
            Ok(insights) => {
                let summaries: Vec<InsightSummary> = insights
                    .iter()
                    .map(|i| InsightSummary {
                        title: i.title.clone(),
                        body: i.body.clone(),
                        relevance: i.relevance,
                        insight_type: crate::graph::models::serialize_insight_type(&i.insight_type),
                    })
                    .collect();
                Response::InsightsResult(summaries)
            }
            Err(e) => Response::Error(format!("insights error: {}", e)),
        },
        Request::GetSessions { limit } => match graph.get_sessions(limit) {
            Ok(sessions) => Response::SessionsResult(sessions),
            Err(e) => Response::Error(format!("sessions error: {}", e)),
        },
        Request::GetRelatedFiles { file_path } => {
            match graph.get_related_files_detailed(&file_path) {
                Ok(entries) => Response::RelatedFilesResult(entries),
                Err(e) => Response::Error(format!("related files error: {}", e)),
            }
        }
        Request::Export => match graph.export_data() {
            Ok(data) => Response::ExportResult(data),
            Err(e) => Response::Error(format!("export error: {}", e)),
        },
        Request::Import { data } => match graph.import_data(&data) {
            Ok(_) => Response::Ok,
            Err(e) => Response::Error(format!("import error: {}", e)),
        },
        Request::Shutdown => {
            tracing::info!("shutdown requested via socket");
            Response::Ok
        }
    }
}
