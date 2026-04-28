/// Retry policy configuration and helpers.
use crate::router::fallback::FallbackDecision;

/// Compute the delay before the next retry attempt using exponential backoff.
pub fn backoff_duration(attempt: u32, base_ms: u64, cap_ms: u64, jitter: bool) -> u64 {
    let delay = base_ms * (1u64 << attempt.min(10)); // cap exponent at 10
    let delay = delay.min(cap_ms);

    if jitter {
        let jitter_amount = delay / 4;
        delay.saturating_sub(jitter_amount.saturating_div(2))
            + (rand::random::<u64>() % jitter_amount.saturating_add(1))
    } else {
        delay
    }
}

/// Classify whether an error should be retried on the same provider.
pub fn should_retry(fallback: &FallbackDecision) -> bool {
    matches!(fallback, FallbackDecision::Retry { .. } | FallbackDecision::RetryWithDelay { .. })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_exponential() {
        let d1 = backoff_duration(1, 100, 5000, false);
        assert_eq!(d1, 200); // 100 * 2^1
    }

    #[test]
    fn test_backoff_capped() {
        let d = backoff_duration(10, 100, 5000, false);
        assert_eq!(d, 5000);
    }
}
