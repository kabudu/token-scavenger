//! `tokenscavenger mlx status` — read-only MLX runtime/server detection.
//!
//! Never installs, downloads, starts, or stops anything. Managing the
//! `mlx_lm.server` process stays an explicit operator action; see
//! `documentation/mlx.md` for the manual install/run steps.

use std::time::Duration;

/// Actions for `tokenscavenger mlx` (all read-only).
#[derive(clap::Subcommand, Debug, Clone)]
pub enum MlxAction {
    /// Show MLX runtime/server detection status.
    Status {
        /// Override the MLX server base URL to probe.
        #[arg(long)]
        base_url: Option<String>,
        /// Emit JSON instead of human-readable text.
        #[arg(long)]
        json: bool,
    },
}

/// Run the MLX status check and print the result.
pub async fn run_mlx_status(
    base_url_override: Option<String>,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let base_url = match base_url_override {
        Some(url) => url,
        None => match crate::cli::find_existing_config() {
            Some(path) => {
                let config = crate::config::loader::load_config(&path)?;
                crate::mlx::mlx_base_url(&config)
            }
            None => crate::mlx::DEFAULT_MLX_BASE_URL.to_string(),
        },
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;
    let status = crate::mlx::detect(&client, &base_url).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("MLX status (read-only)");
    println!(
        "  Platform:        {}",
        if status.supported_platform {
            "Apple Silicon macOS (supported)"
        } else {
            "not Apple Silicon macOS (MLX unsupported here)"
        }
    );
    println!(
        "  Runtime:         {} ({})",
        if status.runtime_installed {
            "installed"
        } else {
            "not installed"
        },
        status.runtime_detail
    );
    println!(
        "  Server:          {} at {}",
        if status.server_reachable {
            "reachable"
        } else {
            "not reachable"
        },
        status.base_url
    );
    if status.server_reachable {
        if status.served_models.is_empty() {
            println!("  Served models:   (none reported)");
        } else {
            println!("  Served models:   {}", status.served_models.join(", "));
        }
    } else {
        println!();
        println!("  To serve a model manually:");
        println!("    {}", status.serve_command);
        println!("  See documentation/mlx.md for install and run steps.");
    }
    Ok(())
}
