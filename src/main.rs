//! TokenScavenger — Lightweight, self-hosted LLM proxy/router.
//!
//! Prioritizes free-tier inference providers and automatically falls back
//! to paid providers when free quota or health conditions require it.
//!
//! Exposes an OpenAI-compatible HTTP API so existing clients can switch
//! by changing only the `base_url`.

use tokenscavenger::api;
use tokenscavenger::app;
use tokenscavenger::config;

use std::path::PathBuf;
use clap::Parser;
use tracing::info;

/// TokenScavenger CLI arguments.
#[derive(Parser, Debug)]
#[command(name = "tokenscavenger", version, about = "LLM proxy/router prioritizing free-tier providers")]
struct Cli {
    /// Path to the configuration file.
    #[arg(short, long, default_value = "tokenscavenger.toml")]
    config: PathBuf,

    /// Database path (overrides config file).
    #[arg(short, long)]
    db: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Initialize metrics global recorder
    let _handle = metrics_exporter_prometheus::PrometheusBuilder::new()
        .install()
        .expect("Failed to install metrics recorder");

    // Bootstrap startup
    let startup_result = app::startup::startup(&cli.config).await?;

    let state = startup_result.state;
    let router = startup_result.router;
    let listener = startup_result.listener;

    info!("TokenScavenger v{} starting on {}", env!("CARGO_PKG_VERSION"), state.config().server.bind);

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
