use crate::config::loader::load_config;
use crate::config::schema::{Config, ProviderConfig};
use dialoguer::{Confirm, Input, Password, Select};
use std::path::Path;
use tracing::info;

/// Run the interactive config editor. Loads existing config, presents sections for editing.
pub fn run_config_editor(config_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let config = match load_config(config_path) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("Error loading config from {}: {}", config_path.display(), e);
            eprintln!("Use `tokenscavenger` (without arguments) for initial setup.");
            std::process::exit(1);
        }
    };

    println!();
    println!("╔══════════════════════════════════════════════════════╗");
    println!("║        TokenScavenger — Configuration Editor        ║");
    println!("╚══════════════════════════════════════════════════════╝");
    println!("Config file: {}", config_path.display());
    println!();

    let mut config = config;
    loop {
        let option = Select::new()
            .with_prompt("Select a section to edit")
            .item("Server settings")
            .item("Database settings")
            .item("Routing settings")
            .item("Resilience settings")
            .item("Providers")
            .item("Save and apply to running server")
            .item("Save to file only")
            .item("Exit without saving")
            .default(0)
            .interact()?;

        match option {
            0 => edit_server(&mut config)?,
            1 => edit_database(&mut config)?,
            2 => edit_routing(&mut config)?,
            3 => edit_resilience(&mut config)?,
            4 => edit_providers(&mut config)?,
            5 => {
                // Save to file, then hot-reload the running server
                write_config(config_path, &config)?;
                println!("✓ Configuration saved to file.");
                try_hot_reload(&config)?;
                return Ok(());
            }
            6 => {
                // Save to file only
                write_config(config_path, &config)?;
                println!("✓ Configuration saved to file.");
                println!("  Run `tokenscavenger config` again to apply to a running server.");
                return Ok(());
            }
            7 => {
                println!("Exiting without saving.");
                return Ok(());
            }
            _ => unreachable!(),
        }
    }
}

fn edit_server(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("── Server Settings (leave blank to keep current) ──");

    let bind: String = Input::new()
        .with_prompt("HTTP bind address")
        .default(config.server.bind.clone())
        .allow_empty(true)
        .interact_text()?;
    if !bind.is_empty() {
        config.server.bind = bind;
    }

    let key: String = Input::new()
        .with_prompt("Master API key (hidden, empty = no change)")
        .default("".into())
        .allow_empty(true)
        .interact_text()?;
    if !key.is_empty() {
        config.server.master_api_key = key;
    }

    let cors_input: String = Input::new()
        .with_prompt("Allowed CORS origins (comma-separated)")
        .default(config.server.allowed_cors_origins.join(","))
        .allow_empty(true)
        .interact_text()?;
    if !cors_input.is_empty() {
        config.server.allowed_cors_origins = cors_input
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    let ui_enabled = Confirm::new()
        .with_prompt("Enable web UI?")
        .default(config.server.ui_enabled)
        .interact()?;
    config.server.ui_enabled = ui_enabled;

    println!("✓ Server settings updated");
    Ok(())
}

fn edit_database(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("── Database Settings ──");

    let path: String = Input::new()
        .with_prompt("Database file path")
        .default(config.database.path.clone())
        .allow_empty(true)
        .interact_text()?;
    if !path.is_empty() {
        config.database.path = path;
    }

    let max_conn: String = Input::new()
        .with_prompt("Max connections")
        .default(config.database.max_connections.to_string())
        .allow_empty(true)
        .interact_text()?;
    if let Ok(n) = max_conn.parse::<u32>() {
        config.database.max_connections = n;
    }

    println!("✓ Database settings updated");
    Ok(())
}

fn edit_routing(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("── Routing Settings ──");

    config.routing.free_first = Confirm::new()
        .with_prompt("Prefer free-tier providers first?")
        .default(config.routing.free_first)
        .interact()?;

    config.routing.allow_paid_fallback = Confirm::new()
        .with_prompt("Allow fallback to paid providers?")
        .default(config.routing.allow_paid_fallback)
        .interact()?;

    let order: String = Input::new()
        .with_prompt("Provider fallback order (comma-separated)")
        .default(config.routing.provider_order.join(", "))
        .allow_empty(true)
        .interact_text()?;
    if !order.is_empty() {
        config.routing.provider_order = order
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
    }

    println!("✓ Routing settings updated");
    Ok(())
}

fn edit_resilience(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("── Resilience Settings ──");

    let retries: String = Input::new()
        .with_prompt("Max retries per provider")
        .default(config.resilience.max_retries_per_provider.to_string())
        .allow_empty(true)
        .interact_text()?;
    if let Ok(n) = retries.parse::<u32>() {
        config.resilience.max_retries_per_provider = n;
    }

    let threshold: String = Input::new()
        .with_prompt("Circuit breaker failure threshold")
        .default(config.resilience.breaker_failure_threshold.to_string())
        .allow_empty(true)
        .interact_text()?;
    if let Ok(n) = threshold.parse::<u32>() {
        config.resilience.breaker_failure_threshold = n;
    }

    let cooldown: String = Input::new()
        .with_prompt("Circuit breaker cooldown (seconds)")
        .default(config.resilience.breaker_cooldown_secs.to_string())
        .allow_empty(true)
        .interact_text()?;
    if let Ok(n) = cooldown.parse::<u64>() {
        config.resilience.breaker_cooldown_secs = n;
    }

    let probe: String = Input::new()
        .with_prompt("Health probe interval (seconds)")
        .default(config.resilience.health_probe_interval_secs.to_string())
        .allow_empty(true)
        .interact_text()?;
    if let Ok(n) = probe.parse::<u64>() {
        config.resilience.health_probe_interval_secs = n;
    }

    println!("✓ Resilience settings updated");
    Ok(())
}

fn edit_providers(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    loop {
        println!("── Providers ({}) ──", config.providers.len());

        let mut items = vec!["Add a provider"];
        let provider_labels: Vec<String> = config
            .providers
            .iter()
            .map(|p| {
                if p.enabled {
                    format!("{} (enabled)", p.id)
                } else {
                    format!("{} (disabled)", p.id)
                }
            })
            .collect();
        for label in &provider_labels {
            items.push(label);
        }
        items.push("Done");

        let choice = Select::new()
            .with_prompt("Select a provider to edit, or add a new one")
            .items(&items)
            .default(0)
            .interact()?;

        if choice == 0 {
            // Add
            add_provider(config)?;
        } else if choice == items.len() - 1 {
            // Done
            break;
        } else {
            // Edit existing
            let idx = choice - 1;
            if edit_one_provider(&mut config.providers[idx])? {
                config.providers.remove(idx);
            }
        }
    }
    Ok(())
}

fn add_provider(config: &mut Config) -> Result<(), Box<dyn std::error::Error>> {
    println!();
    println!("  New Provider");

    let id: String = Input::new()
        .with_prompt("  Provider ID (e.g. groq, google, openrouter)")
        .interact_text()?;

    let api_key: String = Password::new()
        .with_prompt("  API key")
        .with_confirmation("  Confirm API key", "Keys do not match")
        .interact()?;

    let free_only = Confirm::new()
        .with_prompt("  Use only free-tier endpoints?")
        .default(true)
        .interact()?;

    let custom_url = Confirm::new()
        .with_prompt("  Custom base URL?")
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

    config.providers.push(ProviderConfig {
        id,
        enabled: true,
        base_url,
        api_key: Some(api_key),
        free_only,
        discover_models: true,
    });

    println!("  ✓ Provider added");
    Ok(())
}

fn edit_one_provider(provider: &mut ProviderConfig) -> Result<bool, Box<dyn std::error::Error>> {
    println!();
    println!("  Editing provider: {}", provider.id);

    let action = Select::new()
        .with_prompt("  Action")
        .item("Toggle enabled/disabled")
        .item("Update API key")
        .item("Change free-only mode")
        .item("Set base URL")
        .item("Remove this provider")
        .item("Back")
        .default(0)
        .interact()?;

    match action {
        0 => {
            provider.enabled = !provider.enabled;
            println!(
                "  Provider {} {}",
                provider.id,
                if provider.enabled {
                    "enabled"
                } else {
                    "disabled"
                }
            );
        }
        1 => {
            let key: String = Password::new()
                .with_prompt("  New API key")
                .with_confirmation("  Confirm API key", "Keys do not match")
                .interact()?;
            provider.api_key = Some(key);
            println!("  ✓ API key updated");
        }
        2 => {
            let val = Confirm::new()
                .with_prompt("  Free-only mode?")
                .default(provider.free_only)
                .interact()?;
            provider.free_only = val;
            println!("  Updated");
        }
        3 => {
            let url: String = Input::new()
                .with_prompt("  Base URL (empty for default)")
                .default(provider.base_url.clone().unwrap_or_default())
                .allow_empty(true)
                .interact_text()?;
            provider.base_url = if url.is_empty() { None } else { Some(url) };
            println!("  ✓ Base URL updated");
        }
        4 => {
            let confirm = Confirm::new()
                .with_prompt("  Really remove this provider?")
                .default(false)
                .interact()?;
            if confirm {
                println!("  Provider {} removed", provider.id);
                return Ok(true);
            }
        }
        _ => {}
    }
    Ok(false)
}

/// After saving config to file, attempt to apply it to a running TokenScavenger instance.
fn try_hot_reload(config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    // Build the base URL from the bind address
    let bind = &config.server.bind;
    let base_url = if bind.contains("://") {
        bind.to_string()
    } else {
        format!("http://{}", bind)
    };

    let health_url = format!("{}/healthz", base_url);
    let config_url = format!("{}/admin/config", base_url);

    println!();
    println!("── Hot-Reload ──");

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    // Ping the server
    match client.get(&health_url).send() {
        Ok(resp) if resp.status().is_success() => {
            println!("  ✓ Running server detected at {}", base_url);
        }
        Ok(resp) => {
            println!(
                "  ✗ Server at {} responded with status {}",
                base_url,
                resp.status()
            );
            println!("  Config saved to file only.");
            return Ok(());
        }
        Err(e) => {
            println!("  ✗ No running server detected at {}: {}", base_url, e);
            println!("  Config saved to file only. Start the server to apply changes.");
            return Ok(());
        }
    }

    // Ask user whether to hot-reload
    let should_reload = dialoguer::Confirm::new()
        .with_prompt("Apply these changes to the running server now?")
        .default(true)
        .interact()?;

    if !should_reload {
        println!("  Changes saved to file only. Apply later from the web UI or by restarting.");
        return Ok(());
    }

    // Serialize config sections for the PUT /admin/config endpoint
    let body = build_reload_payload(config);

    // Build request with optional master API key
    let mut req = client.put(&config_url).json(&body);
    if !config.server.master_api_key.is_empty() {
        req = req.header(
            "Authorization",
            format!("Bearer {}", config.server.master_api_key),
        );
    }

    match req.send() {
        Ok(resp) if resp.status().is_success() => {
            println!("  ✓ Hot-reload successful — running server updated without restart.");
        }
        Ok(resp) => {
            let status = resp.status();
            let text = resp.text().unwrap_or_default();
            println!("  ✗ Hot-reload failed (HTTP {}): {}", status, text);
        }
        Err(e) => {
            println!("  ✗ Failed to send config to server: {}", e);
        }
    }

    Ok(())
}

/// Build the JSON payload for PUT /admin/config from the full config.
fn build_reload_payload(config: &Config) -> serde_json::Value {
    serde_json::json!({
        "server": {
            "bind": config.server.bind,
            "master_api_key": config.server.master_api_key,
            "allowed_cors_origins": config.server.allowed_cors_origins,
            "ui_enabled": config.server.ui_enabled,
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

fn write_config(path: &Path, config: &Config) -> Result<(), Box<dyn std::error::Error>> {
    let toml_string = toml::to_string_pretty(config)?;
    let header = format!(
        r#"# TokenScavenger Configuration
# Last edited by `tokenscavenger config` on {date}
# Environment variables are expanded: ${{VAR_NAME}} or $ENV_VAR_NAME

"#,
        date = chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    std::fs::write(path, header + &toml_string)?;
    info!("Configuration written to {}", path.display());
    Ok(())
}
