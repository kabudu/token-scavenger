//! TokenScavenger — Lightweight, self-hosted LLM proxy/router.
//!
//! Prioritizes free-tier inference providers and automatically falls back
//! to paid providers when free quota or health conditions require it.
//!
//! Exposes an OpenAI-compatible HTTP API so existing clients can switch
//! by changing only the `base_url`.

use tokenscavenger::app;
use tokenscavenger::cli::{config_cmd, setup};
use tokenscavenger::config::schema::Config;

use clap::Parser;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::time::Duration;
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
    /// Manage the system service (install/uninstall).
    Service {
        #[command(subcommand)]
        action: tokenscavenger::cli::ServiceAction,
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
        Some(Command::Service { action }) => {
            tokenscavenger::cli::service::handle_service_command(*action)?;
            return Ok(());
        }
        None => {
            // Normal server start — proceed below.
        }
    }

    // Resolve config path: explicit --config flag, or auto-detect, or run setup.
    let mut setup_config = None;
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
                    let generated_config = setup::run_setup_wizard(&target)?;
                    setup_config = Some(generated_config);
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

    // Bootstrap startup
    let startup_result = match app::startup::startup(&config_path).await {
        Ok(result) => result,
        Err(error) => {
            if let Some(io_error) = error.downcast_ref::<std::io::Error>() {
                if io_error.kind() == ErrorKind::AddrInUse {
                    if let Some(config) = setup_config.as_ref() {
                        match apply_config_to_running_instance(config).await {
                            Ok(base_url) => {
                                println!();
                                println!("TokenScavenger is already running at {}.", base_url);
                                println!(
                                    "Applied your setup changes to the running server without a restart."
                                );
                                println!("Open {}/ui to continue.", base_url);
                                return Ok(());
                            }
                            Err(reload_error) => {
                                eprintln!();
                                eprintln!(
                                    "Setup was saved to {}, but TokenScavenger could not apply it to the running server: {}",
                                    config_path.display(),
                                    reload_error
                                );
                            }
                        }
                    }
                    eprintln!();
                    eprintln!(
                        "The configured address is already in use. If TokenScavenger is already running, open its admin UI and use the config page to apply changes."
                    );
                    return Ok(());
                }
            }
            return Err(error);
        }
    };

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

async fn apply_config_to_running_instance(
    config: &Config,
) -> Result<String, Box<dyn std::error::Error>> {
    let base_url = local_base_url(&config.server.bind);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let health = client
        .get(format!("{base_url}/healthz"))
        .send()
        .await
        .map_err(|error| {
            std::io::Error::new(
                ErrorKind::ConnectionRefused,
                format!("could not reach {base_url}: {error}"),
            )
        })?;
    if !health.status().is_success() {
        return Err(std::io::Error::other(format!(
            "{base_url}/healthz returned HTTP {}",
            health.status()
        ))
        .into());
    }

    let mut request = client
        .put(format!("{base_url}/admin/config"))
        .json(&build_reload_payload(config));
    if !config.server.master_api_key.is_empty() {
        request = request.header(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", config.server.master_api_key),
        );
    }

    let response = request.send().await?;
    if response.status().is_success() {
        Ok(base_url)
    } else {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        Err(std::io::Error::other(format!("/admin/config returned HTTP {status}: {body}")).into())
    }
}

fn local_base_url(bind: &str) -> String {
    let (scheme, authority) = if let Some(rest) = bind.strip_prefix("http://") {
        ("http", rest)
    } else if let Some(rest) = bind.strip_prefix("https://") {
        ("https", rest)
    } else {
        ("http", bind)
    };
    let authority = authority.split('/').next().unwrap_or(authority);
    let authority = if let Some(port) = authority.strip_prefix("0.0.0.0:") {
        format!("127.0.0.1:{port}")
    } else if authority == "0.0.0.0" {
        "127.0.0.1".to_string()
    } else if let Some(port) = authority.strip_prefix("[::]:") {
        format!("127.0.0.1:{port}")
    } else if authority == "[::]" || authority == "::" {
        "127.0.0.1".to_string()
    } else {
        authority.to_string()
    };
    format!("{scheme}://{authority}")
}

fn build_reload_payload(config: &Config) -> serde_json::Value {
    serde_json::json!({
        "server": {
            "bind": config.server.bind,
            "master_api_key": config.server.master_api_key,
            "allowed_cors_origins": config.server.allowed_cors_origins,
            "allow_query_api_keys": config.server.allow_query_api_keys,
            "ui_session_auth": config.server.ui_session_auth,
            "ui_enabled": config.server.ui_enabled,
            "ui_path": config.server.ui_path,
            "request_timeout_ms": config.server.request_timeout_ms,
        },
        "routing": {
            "free_first": config.routing.free_first,
            "allow_paid_fallback": config.routing.allow_paid_fallback,
            "provider_order": config.routing.provider_order,
        },
        "resilience": {
            "max_retries_per_provider": config.resilience.max_retries_per_provider,
            "breaker_failure_threshold": config.resilience.breaker_failure_threshold,
            "breaker_cooldown_secs": config.resilience.breaker_cooldown_secs,
            "health_probe_interval_secs": config.resilience.health_probe_interval_secs,
        },
        "providers": config.providers.iter().map(|p| {
            serde_json::json!({
                "id": p.id,
                "enabled": p.enabled,
                "api_key": p.api_key.as_deref().unwrap_or(""),
                "base_url": p.base_url.as_deref().unwrap_or(""),
                "free_only": p.free_only,
            })
        }).collect::<Vec<_>>(),
    })
}
