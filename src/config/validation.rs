use crate::config::schema::Config;
use reqwest::header::{HeaderName, HeaderValue};
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
    if cfg.server.external_identity.enabled {
        for (field, value) in [
            (
                "server.external_identity.user_header",
                &cfg.server.external_identity.user_header,
            ),
            (
                "server.external_identity.email_header",
                &cfg.server.external_identity.email_header,
            ),
            (
                "server.external_identity.name_header",
                &cfg.server.external_identity.name_header,
            ),
            (
                "server.external_identity.groups_header",
                &cfg.server.external_identity.groups_header,
            ),
        ] {
            if value.parse::<HeaderName>().is_err() {
                v.errors
                    .push(format!("{field} must be a valid HTTP header name"));
            }
        }
        let has_role_group = !cfg.server.external_identity.read_only_groups.is_empty()
            || !cfg.server.external_identity.operator_groups.is_empty()
            || !cfg.server.external_identity.config_editor_groups.is_empty()
            || !cfg
                .server
                .external_identity
                .credential_manager_groups
                .is_empty()
            || !cfg.server.external_identity.admin_groups.is_empty();
        if !has_role_group {
            v.warnings.push(
                "server.external_identity.enabled is true but no role groups are configured"
                    .to_string(),
            );
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

    if cfg.security.credential_encryption.enabled
        && cfg.security.credential_encryption.key_env.trim().is_empty()
    {
        v.errors
            .push("security.credential_encryption.key_env must not be empty".to_string());
    }

    for (field, value) in [
        ("retention.usage_days", cfg.retention.usage_days),
        (
            "retention.health_event_days",
            cfg.retention.health_event_days,
        ),
        ("retention.audit_days", cfg.retention.audit_days),
        (
            "retention.request_trace_days",
            cfg.retention.request_trace_days,
        ),
    ] {
        if value == 0 {
            v.errors.push(format!("{field} must be > 0"));
        }
    }

    if cfg.updates.enabled {
        if cfg.updates.github_repo.split('/').count() != 2 {
            v.errors
                .push("updates.github_repo must be in owner/repo form".to_string());
        }
        if cfg.updates.check_interval_secs < 300 {
            v.errors
                .push("updates.check_interval_secs must be at least 300".to_string());
        }
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

    // Validate routing budgets
    if let Some(limit) = cfg.routing.budgets.max_cost_per_request_usd {
        if limit < 0.0 {
            v.errors
                .push("routing.budgets.max_cost_per_request_usd must be >= 0".to_string());
        }
    }
    if let Some(limit) = cfg.routing.budgets.max_cost_per_day_usd {
        if limit < 0.0 {
            v.errors
                .push("routing.budgets.max_cost_per_day_usd must be >= 0".to_string());
        }
    }
    for (provider, limit) in &cfg.routing.budgets.max_cost_per_provider_per_day_usd {
        if *limit < 0.0 {
            v.errors.push(format!(
                "routing.budgets.max_cost_per_provider_per_day_usd.{provider} must be >= 0"
            ));
        }
    }
    for (model_group, limit) in &cfg.routing.budgets.max_cost_per_model_group_per_day_usd {
        if *limit < 0.0 {
            v.errors.push(format!(
                "routing.budgets.max_cost_per_model_group_per_day_usd.{model_group} must be >= 0"
            ));
        }
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

    validate_agent_routing(cfg, &mut v);

    v
}

fn validate_agent_routing(cfg: &Config, v: &mut ConfigValidation) {
    let agent = &cfg.routing.agent;
    if agent.session_idle_ttl_seconds == 0 {
        v.errors
            .push("routing.agent.session_idle_ttl_seconds must be > 0".into());
    }
    if agent.session_max_lifetime_seconds == 0 {
        v.errors
            .push("routing.agent.session_max_lifetime_seconds must be > 0".into());
    }
    if agent.session_idle_ttl_seconds > agent.session_max_lifetime_seconds {
        v.errors.push(
            "routing.agent.session_idle_ttl_seconds must not exceed session_max_lifetime_seconds"
                .into(),
        );
    }
    if agent.max_sessions == 0 || agent.max_sessions > 10_000 {
        v.errors
            .push("routing.agent.max_sessions must be between 1 and 10000".into());
    }
    if agent.max_sessions_per_project == 0 || agent.max_sessions_per_project > agent.max_sessions {
        v.errors.push(
            "routing.agent.max_sessions_per_project must be between 1 and max_sessions".into(),
        );
    }
    if agent.max_candidates == 0 || agent.max_candidates > 256 {
        v.errors
            .push("routing.agent.max_candidates must be between 1 and 256".into());
    }
    if agent.enabled && agent.profiles.is_empty() {
        v.errors.push(
            "routing.agent.enabled requires at least one routing.agent.profiles entry".into(),
        );
    }

    let mut visiting = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    for (name, profile) in &agent.profiles {
        if name.trim().is_empty() || name.len() > 64 {
            v.errors.push(format!(
                "routing.agent profile name '{name}' must be 1–64 characters"
            ));
        }
        for group in profile.groups() {
            if group.trim().is_empty() {
                v.errors.push(format!(
                    "routing.agent.profiles.{name} has an empty tier group"
                ));
            }
            if group == name {
                v.errors.push(format!(
                    "routing.agent.profiles.{name} group '{group}' points at its own profile"
                ));
            }
        }
        if profile_cycle(agent, name, &mut visiting, &mut visited) {
            v.errors.push(format!(
                "routing.agent.profiles.{name} participates in a profile cycle"
            ));
        }
    }

    for (index, rule) in agent.rules.iter().enumerate() {
        if !agent.profiles.contains_key(&rule.profile) {
            v.errors.push(format!(
                "routing.agent.rules[{index}] references unknown profile '{}'",
                rule.profile
            ));
        }
        if let Some(task) = &rule.task_type {
            if task.is_empty() || task.len() > 64 {
                v.errors.push(format!(
                    "routing.agent.rules[{index}].task_type must be 1–64 characters when set"
                ));
            }
        }
        if let Some(phase) = &rule.phase {
            if !matches!(
                phase.as_str(),
                "auto" | "planner" | "tool_result" | "finalize" | "initial"
            ) {
                v.errors.push(format!(
                    "routing.agent.rules[{index}].phase is not a known phase"
                ));
            }
        }
        if rule
            .min_input_bytes
            .is_some_and(|min| rule.max_input_bytes.is_some_and(|max| min > max))
        {
            v.errors.push(format!(
                "routing.agent.rules[{index}] min_input_bytes exceeds max_input_bytes"
            ));
        }
    }

    let classifier = &agent.classifier;
    if classifier.enabled {
        if classifier.provider_id.trim().is_empty() || classifier.model_id.trim().is_empty() {
            v.errors
                .push("routing.agent.classifier.enabled requires provider_id and model_id".into());
        }
        if !classifier.provider_id.is_empty()
            && !cfg
                .providers
                .iter()
                .any(|provider| provider.id == classifier.provider_id)
        {
            v.errors.push(format!(
                "routing.agent.classifier.provider_id '{}' is not a configured provider",
                classifier.provider_id
            ));
        }
        if !(0.0..=1.0).contains(&classifier.confidence_threshold)
            || !classifier.confidence_threshold.is_finite()
        {
            v.errors.push(
                "routing.agent.classifier.confidence_threshold must be between 0 and 1".into(),
            );
        }
        if classifier.timeout_ms == 0 || classifier.timeout_ms > 5_000 {
            v.errors
                .push("routing.agent.classifier.timeout_ms must be between 1 and 5000".into());
        }
        if classifier.max_input_bytes == 0 || classifier.max_input_bytes > 8192 {
            v.errors
                .push("routing.agent.classifier.max_input_bytes must be between 1 and 8192".into());
        }
        if classifier.max_output_tokens == 0 || classifier.max_output_tokens > 128 {
            v.errors.push(
                "routing.agent.classifier.max_output_tokens must be between 1 and 128".into(),
            );
        }
        if classifier.max_concurrency == 0 || classifier.max_concurrency > 32 {
            v.errors
                .push("routing.agent.classifier.max_concurrency must be between 1 and 32".into());
        }
        if classifier.max_concurrency_per_project == 0
            || classifier.max_concurrency_per_project > classifier.max_concurrency
        {
            v.errors.push(
                "routing.agent.classifier.max_concurrency_per_project must be between 1 and max_concurrency"
                    .into(),
            );
        }
        if classifier.cache_capacity == 0 || classifier.cache_capacity > 10_000 {
            v.errors
                .push("routing.agent.classifier.cache_capacity must be between 1 and 10000".into());
        }
        if classifier.cache_ttl_seconds == 0 {
            v.errors
                .push("routing.agent.classifier.cache_ttl_seconds must be > 0".into());
        }
        if !(0.0..=1.0).contains(&classifier.sample_rate) || !classifier.sample_rate.is_finite() {
            v.errors
                .push("routing.agent.classifier.sample_rate must be between 0 and 1".into());
        }
        if classifier.allowed_project_ids.is_empty() {
            v.warnings.push(
                "routing.agent.classifier.enabled but allowed_project_ids is empty, so no project can classify"
                    .into(),
            );
        }
    }
}

fn profile_cycle(
    agent: &crate::config::schema::AgentRoutingConfig,
    name: &str,
    visiting: &mut std::collections::HashSet<String>,
    visited: &mut std::collections::HashSet<String>,
) -> bool {
    if visited.contains(name) {
        return false;
    }
    if !visiting.insert(name.to_string()) {
        return true;
    }
    if let Some(profile) = agent.profiles.get(name) {
        for group in profile.groups() {
            if agent.profiles.contains_key(group) && profile_cycle(agent, group, visiting, visited)
            {
                return true;
            }
        }
    }
    visiting.remove(name);
    visited.insert(name.to_string());
    false
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
                    embedding_support: Default::default(),
                },
                ProviderConfig {
                    id: "groq".into(),
                    enabled: true,
                    base_url: None,
                    api_key: None,
                    free_only: true,
                    discover_models: true,
                    embedding_support: Default::default(),
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

    #[test]
    fn test_validate_rejects_invalid_external_identity_header_names() {
        let cfg = Config {
            server: ServerConfig {
                external_identity: ExternalIdentityConfig {
                    enabled: true,
                    user_header: "bad header".into(),
                    admin_groups: vec!["admins".into()],
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };
        let result = validate_config(&cfg);
        assert!(
            result
                .errors
                .iter()
                .any(|error| error.contains("external_identity.user_header"))
        );
    }

    #[test]
    fn test_validate_rejects_negative_routing_budget() {
        let mut cfg = Config::default();
        cfg.routing.budgets.max_cost_per_request_usd = Some(-0.01);
        cfg.routing
            .budgets
            .max_cost_per_provider_per_day_usd
            .insert("deepseek".into(), -1.0);

        let result = validate_config(&cfg);

        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("max_cost_per_request_usd"))
        );
        assert!(
            result
                .errors
                .iter()
                .any(|e| e.contains("max_cost_per_provider_per_day_usd.deepseek"))
        );
    }
}
