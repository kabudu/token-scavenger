use crate::config::schema::{
    Config, DatabaseConfig, LoggingConfig, MetricsConfig, ProviderConfig, ResilienceConfig,
    RoutingConfig, ServerConfig,
};
use console::{Style, style};
use dialoguer::{Confirm, Input, MultiSelect, Password};
use std::path::Path;
use tracing::info;

/// Run the first-time setup wizard. Creates a config file, then returns the path.
pub fn run_setup_wizard(target_path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    let orange = Style::new().for_stderr().color256(208).bold(); // Orange-ish
    let cyan = Style::new().for_stderr().cyan().bold();
    let emerald = Style::new().for_stderr().green().bold();

    println!();
    println!(
        "{}",
        cyan.apply_to("╔══════════════════════════════════════════════════════╗")
    );
    println!(
        "║        {}{} — Setup Wizard               ║",
        cyan.apply_to("Token"),
        orange.apply_to("Scavenger")
    );
    println!(
        "{}",
        cyan.apply_to("╚══════════════════════════════════════════════════════╝")
    );
    println!();
    println!(
        "  {}",
        style("Welcome! Let's get your local LLM proxy running.").italic()
    );
    println!(
        "  {}",
        style("These settings can be managed later via the Web UI.").italic()
    );
    println!();

    // --- Server settings ---
    println!("{}", cyan.apply_to("── Server Settings ──"));
    let bind: String = Input::new()
        .with_prompt("HTTP bind address")
        .default("0.0.0.0:8000".into())
        .interact_text()?;

    let use_master_key = Confirm::new()
        .with_prompt("Require an API key for all requests?")
        .default(false)
        .interact()?;

    let master_api_key = if use_master_key {
        Password::new()
            .with_prompt("Enter master API key")
            .with_confirmation("Confirm master API key", "Keys do not match")
            .interact()?
    } else {
        String::new()
    };

    // --- Database settings ---
    println!();
    println!("{}", cyan.apply_to("── Database Settings ──"));
    let default_db = crate::cli::default_config_path()
        .parent()
        .map(|p| p.join("tokenscavenger.db"))
        .unwrap_or_else(|| std::path::PathBuf::from("tokenscavenger.db"))
        .to_string_lossy()
        .to_string();
    let db_path: String = Input::new()
        .with_prompt("Database file path")
        .default(default_db)
        .interact_text()?;

    // --- Routing settings ---
    println!();
    println!("{}", cyan.apply_to("── Routing Settings ──"));
    let free_first = Confirm::new()
        .with_prompt("Prefer free-tier providers first?")
        .default(true)
        .interact()?;

    let allow_paid = Confirm::new()
        .with_prompt("Allow fallback to paid providers when free quota is exhausted?")
        .default(false)
        .interact()?;

    // --- Provider setup ---
    println!();
    println!("{}", orange.apply_to("── Provider Setup ──"));
    println!("TokenScavenger supports 14+ free and paid LLM providers.");
    println!("Select the providers you'd like to configure now.");

    let available_providers = vec![
        "groq",
        "google (Gemini)",
        "openrouter",
        "cerebras",
        "mistral",
        "nvidia (NIM)",
        "cloudflare (Workers AI)",
        "huggingface (Inference API)",
        "cohere",
        "github (Models)",
        "zhipu (ZAI)",
        "siliconflow",
        "deepseek",
        "xai (Grok)",
    ];

    let chosen = MultiSelect::new()
        .with_prompt("Use space to select, Enter to confirm")
        .items(&available_providers)
        .interact()?;

    let mut providers: Vec<ProviderConfig> = Vec::new();

    for idx in chosen {
        let raw_id = available_providers[idx];
        let id = provider_id_from_label(raw_id).to_string();
        println!();
        println!("  Configuring {}:", cyan.apply_to(&id));

        let api_key: String = Password::new()
            .with_prompt("  API key")
            .with_confirmation("  Confirm API key", "Keys do not match")
            .interact()?;

        let free_only = Confirm::new()
            .with_prompt("  Use only free-tier endpoints?")
            .default(default_free_only_for_provider(&id))
            .interact()?;

        let custom_url = Confirm::new()
            .with_prompt("  Use a custom base URL?")
            .default(false)
            .interact()?;

        let base_url = if custom_url {
            Some(
                Input::<String>::new()
                    .with_prompt("  Base URL")
                    .interact_text()?,
            )
        } else {
            None
        };

        providers.push(ProviderConfig {
            id,
            enabled: true,
            base_url,
            api_key: Some(api_key),
            free_only,
            discover_models: true,
        });
    }

    // --- Build the config ---
    let config = Config {
        version: env!("CARGO_PKG_VERSION").to_string(),
        server: ServerConfig {
            bind: bind.clone(),
            master_api_key,
            allowed_cors_origins: vec![],
            allow_query_api_keys: false,
            ui_session_auth: false,
            ui_enabled: true,
            ui_path: "/ui".into(),
            request_timeout_ms: 120_000,
        },
        database: DatabaseConfig {
            path: db_path,
            max_connections: 8,
        },
        logging: LoggingConfig {
            format: "json".into(),
            level: "info".into(),
        },
        metrics: MetricsConfig {
            enabled: true,
            path: "/metrics".into(),
        },
        routing: RoutingConfig {
            free_first,
            allow_paid_fallback: allow_paid,
            default_alias_strategy: "provider-priority".into(),
            provider_order: providers.iter().map(|p| p.id.clone()).collect(),
        },
        resilience: ResilienceConfig {
            max_retries_per_provider: 2,
            breaker_failure_threshold: 3,
            breaker_cooldown_secs: 60,
            health_probe_interval_secs: 30,
        },
        providers,
    };

    // --- Write the config ---
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let toml_string = toml::to_string_pretty(&config)?;
    let header = format!(
        r#"# TokenScavenger Configuration
# Generated by setup wizard on {date}
# Edit this file directly or use `tokenscavenger config` to reconfigure.
# Environment variables are expanded: ${{VAR_NAME}} or $ENV_VAR_NAME

"#,
        date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );

    std::fs::write(target_path, header + &toml_string)?;

    println!();
    println!(
        "{} Configuration saved to: {}",
        emerald.apply_to("✓"),
        style(target_path.display()).bold()
    );
    println!();
    println!(
        "You can now start {}{}:",
        cyan.apply_to("Token"),
        orange.apply_to("Scavenger")
    );
    println!(
        "  tokenscavenger --config {}",
        style(target_path.display()).underlined()
    );
    println!();
    println!("To reconfigure later use the web UI:");
    println!(
        "  {}",
        style(format!("http://{bind}/ui", bind = bind))
            .cyan()
            .underlined()
    );
    println!();

    info!(
        "Setup wizard completed — config written to {}",
        target_path.display()
    );

    Ok(config)
}

/// Check if any standard config file exists.
pub fn has_existing_config() -> bool {
    crate::cli::find_existing_config().is_some()
}

pub(crate) fn provider_id_from_label(label: &str) -> &str {
    match label {
        "google (Gemini)" => "google",
        "nvidia (NIM)" => "nvidia",
        "cloudflare (Workers AI)" => "cloudflare",
        "huggingface (Inference API)" => "huggingface",
        "github (Models)" => "github-models",
        "zhipu (ZAI)" => "zai",
        "xai (Grok)" => "xai",
        other => other,
    }
}

pub(crate) fn default_free_only_for_provider(provider_id: &str) -> bool {
    !matches!(provider_id, "deepseek" | "xai")
}

#[cfg(test)]
mod tests {
    use super::{default_free_only_for_provider, provider_id_from_label};

    #[test]
    fn setup_labels_map_to_registry_ids() {
        assert_eq!(provider_id_from_label("github (Models)"), "github-models");
        assert_eq!(provider_id_from_label("zhipu (ZAI)"), "zai");
        assert_eq!(provider_id_from_label("xai (Grok)"), "xai");
        assert_eq!(provider_id_from_label("deepseek"), "deepseek");
        assert_eq!(provider_id_from_label("google (Gemini)"), "google");
        assert_eq!(provider_id_from_label("groq"), "groq");
    }

    #[test]
    fn paid_fallback_providers_default_to_paid_mode() {
        assert!(!default_free_only_for_provider("deepseek"));
        assert!(!default_free_only_for_provider("xai"));
        assert!(default_free_only_for_provider("groq"));
    }
}
