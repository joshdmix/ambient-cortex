mod commands;

use anyhow::Result;
use clap::Parser;
use commands::{config, export, history, import, install, query, search, status, tmux, tui};

#[derive(Parser)]
#[command(name = "cortex", about = "Ambient Cortex CLI", version)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Show daemon status and health info
    Status,
    /// Query context for a specific file
    Query {
        /// Path to the file to query
        file_path: String,
    },
    /// Show recent event history
    History {
        /// Maximum number of events to show
        #[arg(long, default_value = "20")]
        limit: usize,
    },
    /// Search across events and insights
    Search {
        /// Search query string
        query: String,
    },
    /// Install cortex: create directories, config, and shell hooks
    Install,
    /// Launch the TUI dashboard
    Tui,
    /// Output status for tmux status bar
    TmuxStatus,
    /// Export all data to a JSON file
    Export {
        /// Output file path
        #[arg(long, default_value = "cortex-backup.json")]
        output: String,
    },
    /// Import data from a JSON backup file
    Import {
        /// Input file path
        #[arg(long)]
        input: String,
    },
    /// View or edit configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
}

#[derive(clap::Subcommand)]
enum ConfigAction {
    /// Open config file in $EDITOR
    Edit,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("cortex=info".parse()?),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Command::Status => status::run().await?,
        Command::Query { file_path } => query::run(&file_path).await?,
        Command::History { limit } => history::run(limit).await?,
        Command::Search { query } => search::run(&query).await?,
        Command::Install => install::run()?,
        Command::Tui => tui::run().await?,
        Command::TmuxStatus => tmux::run()?,
        Command::Export { output } => export::run(&output).await?,
        Command::Import { input } => import::run(&input).await?,
        Command::Config { action } => match action {
            Some(ConfigAction::Edit) => config::edit()?,
            None => config::show()?,
        },
    }

    Ok(())
}
