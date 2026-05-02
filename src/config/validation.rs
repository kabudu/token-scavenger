use crate::config::schema::Config;
use reqwest::header::HeaderValue;
use url::Url;

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
    for origin in &cfg.server.allowed_cors_origins {
        if origin.parse::<HeaderValue>().is_err() {
            v.errors.push(format!(
                "server.allowed_cors_origins contains an invalid header value: {origin}"
            ));
        }
    }

    // Validate database
    if cfg.database.path.is_empty() {
        v.errors.push("database.path must not be empty".to_string());
    }
    if cfg.database.max_connections == 0 {
        v.errors
            .push("database.max_connections must be > 0".to_string());
    }

    // Validate resilience
    if cfg.resilience.max_retries_per_provider > 10 {
        v.warnings
            .push("resilience.max_retries_per_provider is high (>10)".to_string());
    }
    if cfg.resilience.breaker_failure_threshold == 0 {
        v.errors
            .push("resilience.breaker_failure_threshold must be > 0".to_string());
    }
    if cfg.resilience.breaker_cooldown_secs == 0 {
        v.errors
            .push("resilience.breaker_cooldown_secs must be > 0".to_string());
    }

    // Validate providers
    let mut provider_ids = std::collections::HashSet::new();
    for provider in &cfg.providers {
        if provider.id.is_empty() {
            v.errors
                .push("A provider entry has an empty id".to_string());
        }
        if !provider_ids.insert(&provider.id) {
            v.errors
                .push(format!("Duplicate provider id: {}", provider.id));
        }
        if let Some(api_key) = &provider.api_key {
            if api_key.parse::<HeaderValue>().is_err() {
                v.errors.push(format!(
                    "provider '{}' api_key cannot be represented as an HTTP header value",
                    provider.id
                ));
            }
        }
        if let Some(base_url) = &provider.base_url {
            if Url::parse(base_url).is_err() {
                v.errors.push(format!(
                    "provider '{}' base_url is invalid: {}",
                    provider.id, base_url
                ));
            }
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
    fn test_validate_database_pool_size() {
        let mut cfg = Config::default();
        cfg.database.max_connections = 0;
        let result = validate_config(&cfg);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("database.max_connections"))
        );
    }

    #[test]
    fn test_validate_duplicate_provider() {
        let cfg = Config {
            providers: vec![
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
            ],
            ..Default::default()
        };
        let result = validate_config(&cfg);
        assert!(result.errors.iter().any(|e| e.contains("Duplicate")));
    }

    #[test]
    fn test_validate_rejects_invalid_header_values() {
        let cfg = Config {
            server: ServerConfig {
                allowed_cors_origins: vec!["https://example.com\nbad".into()],
                ..Default::default()
            },
            providers: vec![ProviderConfig {
                id: "groq".into(),
                api_key: Some("bad\nkey".into()),
                ..Default::default()
            }],
            ..Default::default()
        };
        let result = validate_config(&cfg);
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("allowed_cors_origins"))
        );
        assert!(result.errors.iter().any(|e| e.contains("api_key")));
    }
}
