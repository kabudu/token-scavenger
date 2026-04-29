//! Structured tracing setup and utilities.
//! The main tracing initialization is done in app::startup.
//! This module provides helpers for consistent log field usage.

/// Create a span for a request with all standard fields.
#[macro_export]
macro_rules! request_span {
    ($request_id:expr, $endpoint:expr, $model:expr) => {
        tracing::info_span!(
            "request",
            request_id = $request_id,
            endpoint_kind = $endpoint,
            requested_model = $model,
            provider_id = tracing::field::Empty,
            latency_ms = tracing::field::Empty,
            http_status = tracing::field::Empty,
        )
    };
}
