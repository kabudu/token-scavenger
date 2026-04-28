use crate::app::state::AppState;
use crate::providers::traits::ProviderError;

/// Fallback engine for handling retries and cross-provider fallback.
pub async fn should_fallback(
    _state: &AppState,
    error: &ProviderError,
) -> FallbackDecision {
    match error {
        // Retryable on same provider
        ProviderError::Timeout => FallbackDecision::Retry { max_attempts: 2 },
        ProviderError::Http(_) => FallbackDecision::Retry { max_attempts: 2 },
        ProviderError::RateLimited { .. } => FallbackDecision::RetryWithDelay { delay_ms: 1000 },

        // Not retryable on same provider, but can try another provider
        ProviderError::Auth(_) => FallbackDecision::TryNextProvider,
        ProviderError::QuotaExhausted { .. } => FallbackDecision::TryNextProvider,
        ProviderError::UnsupportedFeature(_) => FallbackDecision::TryNextProvider,
        ProviderError::UnknownModel(_) => FallbackDecision::TryNextProvider,

        // Not recoverable on any provider
        ProviderError::MalformedResponse(_) => FallbackDecision::Fail,
        ProviderError::Other(_) => FallbackDecision::Retry { max_attempts: 1 },
    }
}

pub enum FallbackDecision {
    /// Retry on the same provider.
    Retry { max_attempts: u32 },
    /// Retry after a delay.
    RetryWithDelay { delay_ms: u64 },
    /// Skip to the next provider in the plan.
    TryNextProvider,
    /// Fail the entire request.
    Fail,
}
