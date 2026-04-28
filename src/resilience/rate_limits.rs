/// Rate limit tracking for providers.
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// In-memory rate limit state per provider.
#[derive(Debug, Clone, Default)]
pub struct ProviderRateLimit {
    pub remaining: Option<u64>,
    pub limit: Option<u64>,
    pub reset_at: Option<i64>,
    pub retry_after: Option<u64>,
}

/// Shared rate limit tracker.
pub struct RateLimitTracker {
    limits: RwLock<HashMap<String, ProviderRateLimit>>,
}

impl RateLimitTracker {
    pub fn new() -> Self {
        Self { limits: RwLock::new(HashMap::new()) }
    }

    pub async fn update(&self, provider_id: &str, info: ProviderRateLimit) {
        self.limits.write().await.insert(provider_id.to_string(), info);
    }

    pub async fn get(&self, provider_id: &str) -> Option<ProviderRateLimit> {
        self.limits.read().await.get(provider_id).cloned()
    }

    pub async fn is_limited(&self, provider_id: &str) -> bool {
        if let Some(info) = self.get(provider_id).await {
            if let Some(remaining) = info.remaining {
                return remaining == 0;
            }
        }
        false
    }
}
