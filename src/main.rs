//! TokenScavenger — Lightweight, self-hosted LLM proxy/router.
//!
//! Prioritizes free-tier inference providers and automatically falls back
//! to paid providers when free quota or health conditions require it.
//!
//! Exposes an OpenAI-compatible HTTP API so existing clients can switch
//! by changing only the `base_url`.

use tokenscavenger::app;
use tokenscavenger::cli::{config_cmd, setup};

use clap::Parser;
use std::path::PathBuf;
use tracing::info;

/// TokenScavenger CLI arguments.
#[derive(Parser, Debug)]
#[command(
    name = "tokenscavenger",
    version,
    about = "LLM proxy/router prioritizing free-tier providers"
)]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long)]
    config: Option<PathBuf>,

    /// Database path (overrides config file).
    #[arg(short, long)]
    db: Option<String>,

    /// Subcommand: run the server (default), run the config editor, or run the setup wizard.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Parser, Debug)]
enum Command {
    /// Interactive configuration editor.
    Config {
        /// Path to the configuration file (default: auto-detect).
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// First-time setup wizard.
    Setup {
        /// Where to write the new configuration file.
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Handle subcommands first — they don't start the server.
    match &cli.command {
        Some(Command::Config { config }) => {
            let cfg_path = resolve_config_path(config.as_ref());
            config_cmd::run_config_editor(&cfg_path)?;
            return Ok(());
        }
        Some(Command::Setup { output }) => {
            let target = output
                .clone()
                .unwrap_or_else(tokenscavenger::cli::default_config_path);
            setup::run_setup_wizard(&target)?;
            return Ok(());
        }
        None => {
            // Normal server start — proceed below.
        }
    }

    // Resolve config path: explicit --config flag, or auto-detect, or run setup.
    let config_path = match &cli.config {
        Some(path) => path.clone(),
        None => match tokenscavenger::cli::find_existing_config() {
            Some(path) => path,
            None => {
                // First run — no config found anywhere.
                let target = tokenscavenger::cli::default_config_path();
                println!("No configuration file found.");
                let run_setup = dialoguer::Confirm::new()
                    .with_prompt("Would you like to run the setup wizard?")
                    .default(true)
                    .interact()?;
                if run_setup {
                    setup::run_setup_wizard(&target)?;
                    target
                } else {
                    println!(
                        "You can create a config manually or run `tokenscavenger setup` later."
                    );
                    println!("Starting with default config (it won't work without providers).");
                    PathBuf::from("tokenscavenger.toml")
                }
            }
        },
    };

    // Initialize metrics global recorder. If the recorder cannot bind the metrics
    // listener (for example because the port is already in use), log a warning and
    // continue starting the server without Prometheus scraping.
    if let Err(err) = metrics_exporter_prometheus::PrometheusBuilder::new().install() {
        tracing::warn!("Failed to install metrics recorder: {}", err);
    }

    // Bootstrap startup
    let startup_result = app::startup::startup(&config_path).await?;

    let state = startup_result.state;
    let router = startup_result.router;
    let listener = startup_result.listener;

    info!(
        "TokenScavenger v{} starting on {}",
        env!("CARGO_PKG_VERSION"),
        state.config().server.bind
    );

    // Override DB path from CLI if provided
    if let Some(db_path) = cli.db {
        info!("Database path overridden by CLI: {}", db_path);
    }

    // Run the server with graceful shutdown
    axum::serve(listener, router)
        .with_graceful_shutdown(async move {
            app::shutdown::shutdown(state.clone()).await;
        })
        .await?;

    Ok(())
}

/// Resolve the config path: explicit path, or auto-detect, or default.
fn resolve_config_path(explicit: Option<&PathBuf>) -> PathBuf {
    explicit
        .cloned()
        .or_else(tokenscavenger::cli::find_existing_config)
        .unwrap_or_else(|| PathBuf::from("tokenscavenger.toml"))
}
