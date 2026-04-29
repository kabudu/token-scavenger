use crate::config::env;
use crate::config::schema::Config;
use crate::config::validation::ConfigValidation;
use crate::config::validation::validate_config;
use std::path::Path;

/// Load configuration from a TOML file, expand env vars, and validate.
pub fn load_config(path: &Path) -> Result<Config, ConfigLoadError> {
    let contents = std::fs::read_to_string(path).map_err(|e| ConfigLoadError::Io(e.to_string()))?;

    let mut cfg: Config =
        toml::from_str(&contents).map_err(|e| ConfigLoadError::Parse(e.to_string()))?;

    // Apply env var expansion
    cfg = env::expand_all(&cfg);

    // Validate
    let validation = validate_config(&cfg);
    if !validation.errors.is_empty() {
        return Err(ConfigLoadError::Validation(validation));
    }

    Ok(cfg)
}

/// Load config from a string (for tests).
pub fn load_config_from_str(s: &str) -> Result<Config, ConfigLoadError> {
    let mut cfg: Config = toml::from_str(s).map_err(|e| ConfigLoadError::Parse(e.to_string()))?;
    cfg = env::expand_all(&cfg);
    let validation = validate_config(&cfg);
    if !validation.errors.is_empty() {
        return Err(ConfigLoadError::Validation(validation));
    }
    Ok(cfg)
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("TOML parse error: {0}")]
    Parse(String),
    #[error("Config validation failed")]
    Validation(ConfigValidation),
}
