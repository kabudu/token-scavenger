use crate::config::schema::Config;

/// Result of config validation.
#[derive(Debug, Default)]
pub struct ConfigValidation {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

/// Validate the configuration, returning errors and warnings.
pub fn validate_config(cfg: &Config) -> ConfigValidation {
    let mut v = ConfigValidation::default();

    // Validate server
    if cfg.server.bind.is_empty() {
        v.errors.push("server.bind must not be empty".to_string());
    }

    // Validate database
    if cfg.database.path.is_empty() {
        v.errors.push("database.path must not be empty".to_string());
    }

    // Validate resilience
    if cfg.resilience.max_retries_per_provider > 10 {
        v.warnings.push("resilience.max_retries_per_provider is high (>10)".to_string());
    }
    if cfg.resilience.breaker_failure_threshold == 0 {
        v.errors.push("resilience.breaker_failure_threshold must be > 0".to_string());
    }
    if cfg.resilience.breaker_cooldown_secs == 0 {
        v.errors.push("resilience.breaker_cooldown_secs must be > 0".to_string());
    }

    // Validate providers
    let mut provider_ids = std::collections::HashSet::new();
    for provider in &cfg.providers {
        if provider.id.is_empty() {
            v.errors.push("A provider entry has an empty id".to_string());
        }
        if !provider_ids.insert(&provider.id) {
            v.errors.push(format!("Duplicate provider id: {}", provider.id));
        }
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::*;

    #[test]
    fn test_validate_empty_bind() {
        let mut cfg = Config::default();
        cfg.server.bind = "".to_string();
        let result = validate_config(&cfg);
        assert!(!result.errors.is_empty());
    }

    #[test]
    fn test_validate_valid_config() {
        let cfg = Config::default();
        let result = validate_config(&cfg);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_validate_duplicate_provider() {
        let mut cfg = Config::default();
        cfg.providers = vec![
            ProviderConfig {
                id: "groq".into(),
                enabled: true,
                base_url: None,
                api_key: None,
                free_only: true,
                discover_models: true,
            },
            ProviderConfig {
                id: "groq".into(),
                enabled: true,
                base_url: None,
                api_key: None,
                free_only: true,
                discover_models: true,
            },
        ];
        let result = validate_config(&cfg);
        assert!(result.errors.iter().any(|e| e.contains("Duplicate")));
    }
}
