use crate::config::env;
use crate::config::schema::Config;
use crate::config::validation::validate_config;
use std::path::Path;
use tracing::info;

/// Path to the runtime overrides sidecar file.
pub fn overrides_path(config_path: &Path) -> std::path::PathBuf {
    let mut p = config_path.to_path_buf();
    let ext = p
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    p.set_extension(format!("overrides.{}", ext));
    p
}

/// Save a config snapshot to the sidecar overrides file so runtime
/// changes survive a restart. On next startup these overrides are
/// merged back into the base config.
pub fn save_runtime_overrides(
    config_path: &Path,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = overrides_path(config_path);
    let toml_str = toml::to_string_pretty(config)?;

    let header = "# TokenScavenger Runtime Overrides\n".to_string();
    std::fs::write(&path, header + &toml_str)?;
    info!("Runtime config saved to {}", path.display());
    Ok(())
}

/// Load runtime overrides from the sidecar file, if it exists.
pub fn load_runtime_overrides(config_path: &Path) -> Option<Config> {
    let path = overrides_path(config_path);
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(&path) {
        Ok(s) => match toml::from_str::<Config>(&s) {
            Ok(raw_cfg) => {
                let cfg = env::expand_all(&raw_cfg);
                let validation = validate_config(&cfg);
                if !validation.errors.is_empty() {
                    tracing::warn!(
                        path = %path.display(),
                        errors = ?validation.errors,
                        "Runtime overrides failed validation and will be ignored"
                    );
                    return None;
                }
                if !validation.warnings.is_empty() {
                    tracing::warn!(
                        path = %path.display(),
                        warnings = ?validation.warnings,
                        "Runtime overrides loaded with warnings"
                    );
                }
                info!("Loaded runtime overrides from {}", path.display());
                Some(cfg)
            }
            Err(e) => {
                tracing::warn!("Failed to parse runtime overrides: {}", e);
                None
            }
        },
        Err(e) => {
            tracing::warn!("Failed to read runtime overrides: {}", e);
            None
        }
    }
}
