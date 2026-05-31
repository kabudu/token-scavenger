use crate::config::schema::{Config, RoutingBudgetConfig, RoutingObjective};
use std::collections::HashMap;

/// Routing policy derived from config.
#[derive(Debug, Clone)]
pub struct RoutePolicy {
    /// Whether to prefer free-tier providers first.
    pub free_first: bool,
    /// Whether paid fallback is allowed.
    pub allow_paid_fallback: bool,
    /// Default policy objective for scoring eligible attempts.
    pub objective: RoutingObjective,
    /// Per-model-group policy objective overrides.
    pub model_group_objectives: HashMap<String, RoutingObjective>,
    /// Hard budget limits.
    pub budgets: RoutingBudgetConfig,
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
            objective: config.routing.objective,
            model_group_objectives: config.routing.model_group_objectives.clone(),
            budgets: config.routing.budgets.clone(),
            provider_order: order,
        }
    }

    pub fn objective_for_model_group(&self, requested_model: &str) -> RoutingObjective {
        self.model_group_objectives
            .get(requested_model)
            .copied()
            .unwrap_or(self.objective)
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
            "deepseek".into(),
            "xai".into(),
        ]
    }
}
