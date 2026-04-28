use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// Circuit breaker state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BreakerState {
    Closed,
    Open,
    HalfOpen,
}

/// Per-provider circuit breaker.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: RwLock<BreakerState>,
    failure_count: AtomicU32,
    failure_threshold: u32,
    cooldown_secs: u64,
    last_failure: RwLock<Option<Instant>>,
    half_open_trials: AtomicU32,
    max_half_open_trials: u32,
}

/// Serializable circuit breaker state for the UI/API.
#[derive(Debug, Clone)]
pub struct CircuitBreakerState {
    state: BreakerState,
    failure_count: u32,
    failure_threshold: u32,
}

impl CircuitBreakerState {
    pub fn new(state: BreakerState, failure_count: u32, failure_threshold: u32) -> Self {
        Self { state, failure_count, failure_threshold }
    }

    pub fn is_open(&self) -> bool {
        self.state == BreakerState::Open || self.state == BreakerState::HalfOpen
    }

    pub fn state(&self) -> BreakerState {
        self.state.clone()
    }
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown_secs: u64) -> Self {
        Self {
            state: RwLock::new(BreakerState::Closed),
            failure_count: AtomicU32::new(0),
            failure_threshold,
            cooldown_secs,
            last_failure: RwLock::new(None),
            half_open_trials: AtomicU32::new(0),
            max_half_open_trials: 3,
        }
    }

    /// Check whether a request can proceed (non-blocking).
    pub async fn allow_request(&self) -> bool {
        let state = self.state.read().await;
        match *state {
            BreakerState::Closed => true,
            BreakerState::Open => {
                // Check if cooldown has elapsed
                if let Some(last) = *self.last_failure.read().await {
                    if last.elapsed().as_secs() >= self.cooldown_secs {
                        drop(state);
                        // Transition to half-open
                        let mut state = self.state.write().await;
                        *state = BreakerState::HalfOpen;
                        self.half_open_trials.store(0, Ordering::SeqCst);
                        return true;
                    }
                }
                false
            }
            BreakerState::HalfOpen => {
                let trials = self.half_open_trials.load(Ordering::SeqCst);
                if trials < self.max_half_open_trials {
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Record a failure and potentially open the breaker.
    pub async fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure.write().await = Some(Instant::now());

        if count >= self.failure_threshold {
            let mut state = self.state.write().await;
            *state = BreakerState::Open;
            self.half_open_trials.store(0, Ordering::SeqCst);
        }
    }

    /// Record a success and potentially close the breaker.
    pub async fn record_success(&self) {
        self.failure_count.store(0, Ordering::SeqCst);
        let mut state = self.state.write().await;
        if *state == BreakerState::HalfOpen {
            let trials = self.half_open_trials.fetch_add(1, Ordering::SeqCst) + 1;
            if trials >= self.max_half_open_trials {
                *state = BreakerState::Closed;
            }
        } else {
            *state = BreakerState::Closed;
        }
    }

    /// Get the current state snapshot for metrics/UI.
    pub async fn snapshot(&self) -> CircuitBreakerState {
        CircuitBreakerState {
            state: self.state.read().await.clone(),
            failure_count: self.failure_count.load(Ordering::SeqCst),
            failure_threshold: self.failure_threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_closed_allows_request() {
        let cb = CircuitBreaker::new(3, 60);
        assert!(cb.allow_request().await);
    }

    #[tokio::test]
    async fn test_opens_after_threshold() {
        let cb = CircuitBreaker::new(2, 60);
        cb.record_failure().await;
        cb.record_failure().await;
        assert!(!cb.allow_request().await);
    }

    #[tokio::test]
    async fn test_success_resets_failures() {
        let cb = CircuitBreaker::new(3, 60);
        cb.record_failure().await;
        cb.record_failure().await;
        cb.record_success().await;
        assert!(cb.allow_request().await);
    }
}
