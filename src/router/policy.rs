use crate::config::schema::Config;

/// Routing policy derived from config.
#[derive(Debug, Clone)]
pub struct RoutePolicy {
    /// Whether to prefer free-tier providers first.
    pub free_first: bool,
    /// Whether paid fallback is allowed.
    pub allow_paid_fallback: bool,
    /// Explicit provider order (from config or default).
    pub provider_order: Vec<String>,
}

impl RoutePolicy {
    pub fn from_config(config: &Config) -> Self {
        let order = if config.routing.provider_order.is_empty() {
            Self::default_order()
        } else {
            config.routing.provider_order.clone()
        };

        Self {
            free_first: config.routing.free_first,
            allow_paid_fallback: config.routing.allow_paid_fallback,
            provider_order: order,
        }
    }

    /// Default provider order from the spec.
    fn default_order() -> Vec<String> {
        vec![
            "groq".into(),
            "cerebras".into(),
            "google".into(),
            "openrouter".into(),
            "cloudflare".into(),
            "nvidia".into(),
            "mistral".into(),
            "github-models".into(),
            "zai".into(),
            "siliconflow".into(),
            "huggingface".into(),
            "cohere".into(),
        ]
    }
}
