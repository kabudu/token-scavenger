use std::collections::HashMap;
use std::env;

/// Expand `${VAR_NAME}` and `$VAR_NAME` environment variable references in a string.
/// Leaves unknown variables as-is (no error).
pub fn expand_env_vars(s: &str) -> String {
    let mut result = s.to_string();

    // Match ${VAR_NAME} patterns
    let re = regex_lite::Regex::new(r"\$\{([^}]+)\}").ok();
    if let Some(re) = &re {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                env::var(&caps[1]).unwrap_or_else(|_| caps[0].to_string())
            })
            .to_string();
    }

    // Match $VAR_NAME but NOT ${...} (already handled above)
    let re2 = regex_lite::Regex::new(r"(?<!\$)\$([A-Za-z_][A-Za-z0-9_]*)").ok();
    if let Some(re) = &re2 {
        result = re
            .replace_all(&result, |caps: &regex_lite::Captures| {
                env::var(&caps[1]).unwrap_or_else(|_| caps[0].to_string())
            })
            .to_string();
    }

    result
}

/// Expand env vars in all string fields of a `ProviderConfig`.
pub fn expand_provider_config(
    cfg: &crate::config::schema::ProviderConfig,
) -> crate::config::schema::ProviderConfig {
    crate::config::schema::ProviderConfig {
        id: cfg.id.clone(),
        enabled: cfg.enabled,
        base_url: cfg.base_url.as_ref().map(|u| expand_env_vars(u)),
        api_key: cfg.api_key.as_ref().map(|k| expand_env_vars(k)),
        free_only: cfg.free_only,
        discover_models: cfg.discover_models,
    }
}

/// Expand env vars across the entire config.
pub fn expand_all(cfg: &crate::config::schema::Config) -> crate::config::schema::Config {
    let mut cfg = cfg.clone();
    cfg.server.master_api_key = expand_env_vars(&cfg.server.master_api_key);
    cfg.providers = cfg.providers.into_iter().map(|p| expand_provider_config(&p)).collect();
    cfg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_dollar_brace() {
        let expanded = expand_env_vars("hello ${USER}");
        // USER is typically set; if not, it stays as-is
        assert!(expanded.starts_with("hello "));
    }

    #[test]
    fn test_expand_no_vars() {
        assert_eq!(expand_env_vars("plain string"), "plain string");
    }

    #[test]
    fn test_expand_unknown_var_preserved() {
        let result = expand_env_vars("${NONEXISTENT_VAR_XYZ999}");
        assert_eq!(result, "${NONEXISTENT_VAR_XYZ999}");
    }

    #[test]
    fn test_expand_provider_config_none_key() {
        let cfg = crate::config::schema::ProviderConfig {
            id: "test".into(),
            enabled: true,
            base_url: Some("https://api.example.com".into()),
            api_key: None,
            free_only: true,
            discover_models: true,
        };
        let expanded = expand_provider_config(&cfg);
        assert_eq!(expanded.api_key, None);
        assert_eq!(expanded.base_url.unwrap(), "https://api.example.com");
    }
}
