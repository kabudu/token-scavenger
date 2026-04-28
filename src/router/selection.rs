use std::sync::Arc;
use crate::providers::registry::ProviderRegistry;
use crate::providers::traits::EndpointKind;
use crate::router::policy::RoutePolicy;

/// A single entry in the attempt plan.
#[derive(Debug, Clone)]
pub struct RouteAttempt {
    pub provider_id: String,
    pub model_id: String,
    pub priority: i32,
}

/// Build an ordered list of provider-model attempts based on the routing policy.
/// Filters by endpoint capability, enablement, and health hints.
pub async fn build_attempt_plan(
    policy: &RoutePolicy,
    registry: &ProviderRegistry,
    model: &str,
    endpoint_kind: EndpointKind,
) -> Vec<RouteAttempt> {
    let mut plan: Vec<RouteAttempt> = Vec::new();

    for provider_id in &policy.provider_order {
        // Get the adapter
        let adapter = match registry.get(provider_id).await {
            Some(a) => a,
            None => continue,
        };

        // Check endpoint support
        if !adapter.supports_endpoint(&endpoint_kind) {
            continue;
        }

        plan.push(RouteAttempt {
            provider_id: provider_id.clone(),
            model_id: model.to_string(),
            priority: plan.len() as i32,
        });
    }

    plan
}
