mod bus;
mod config;
mod engine;
mod graph;
mod insight_writer;
mod server;
mod watchers;

use anyhow::Result;
use std::sync::Arc;
use std::time::Instant;

use bus::EventBus;
use config::CortexConfig;
use graph::store::Store;
use graph::KnowledgeGraph;
use watchers::WatcherManager;

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tracing::info!("cortexd starting up");

    // Load configuration
    let config = CortexConfig::load()?;
    let config = Arc::new(config);

    // Create data directories
    let data_dir = CortexConfig::data_dir();
    std::fs::create_dir_all(&data_dir)?;
    tracing::info!("data directory: {}", data_dir.display());

    // Initialize SQLite store
    let db_path = data_dir.join("cortex.db");
    let store = Store::new(&db_path)?;
    tracing::info!("database initialized at {}", db_path.display());

    // Create the knowledge graph
    let graph = Arc::new(KnowledgeGraph::new(store));

    // Initialize embeddings engine (heavy model load, do it async)
    let embed_graph = graph.clone();
    tokio::spawn(async move {
        match embed_graph.init_embeddings() {
            Ok(()) => tracing::info!("embedding engine initialized"),
            Err(e) => tracing::warn!("embedding engine unavailable: {}", e),
        }
    });

    // Create event bus
    let bus = EventBus::new(1024);

    // Start the inference engine
    let engine_rx = bus.subscribe();
    let engine_graph = graph.clone();
    let insight_threshold = config.insight_threshold;
    let claude_enabled = config.claude_enabled;
    let claude_api_key = config.claude_api_key.clone();
    let claude_max_calls = config.claude_max_calls_per_hour;
    let engine_handle = tokio::spawn(async move {
        let mut engine = engine::InferenceEngine::new(engine_graph, insight_threshold);
        if claude_enabled {
            if let Some(api_key) = claude_api_key {
                engine = engine.with_claude(api_key, claude_max_calls);
                tracing::info!("claude API enabled for inference");
            }
        }
        if let Err(e) = engine.run(engine_rx).await {
            tracing::error!("inference engine error: {}", e);
        }
    });

    // Start event ingestion loop (bus -> graph)
    let ingest_rx = bus.subscribe();
    let ingest_graph = graph.clone();
    let ingest_handle = tokio::spawn(async move {
        ingest_events(ingest_rx, ingest_graph).await;
    });

    // Start watchers
    let mut watcher_manager = WatcherManager::new();
    watcher_manager.start_all(config.clone(), bus.clone())?;
    let watcher_manager = Arc::new(std::sync::Mutex::new(watcher_manager));

    // Start insight writer (writes current_insight.json for shell prompt)
    let insight_graph = graph.clone();
    let insight_handle = tokio::spawn(async move {
        insight_writer::run(insight_graph).await;
    });

    // Shutdown signal
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Start Unix socket server
    let server_graph = graph.clone();
    let server_wm = watcher_manager.clone();
    let start_time = Instant::now();
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server::run(server_graph, server_wm, start_time, shutdown_rx).await {
            tracing::error!("server error: {}", e);
        }
    });

    // Wait for shutdown signal (SIGTERM or SIGINT)
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received SIGINT, shutting down");
        }
        _ = async {
            #[cfg(unix)]
            {
                let mut sigterm = tokio::signal::unix::signal(
                    tokio::signal::unix::SignalKind::terminate()
                ).expect("failed to register SIGTERM handler");
                sigterm.recv().await;
            }
            #[cfg(not(unix))]
            {
                // On non-unix, just wait forever (ctrl_c will handle it)
                std::future::pending::<()>().await;
            }
        } => {
            tracing::info!("received SIGTERM, shutting down");
        }
    }

    // Signal shutdown
    let _ = shutdown_tx.send(true);

    // Clean up socket file
    let socket_path = CortexConfig::socket_path();
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    // Abort long-running tasks
    engine_handle.abort();
    ingest_handle.abort();
    server_handle.abort();
    insight_handle.abort();

    tracing::info!("cortexd shut down cleanly");
    Ok(())
}

async fn ingest_events(
    mut rx: tokio::sync::broadcast::Receiver<cortex_common::events::CortexEvent>,
    graph: Arc<KnowledgeGraph>,
) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Err(e) = graph.ingest_event(&event) {
                    tracing::error!("failed to ingest event: {}", e);
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("ingest loop lagged, skipped {} events", n);
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                tracing::info!("event bus closed, ingest loop stopping");
                break;
            }
        }
    }
}
