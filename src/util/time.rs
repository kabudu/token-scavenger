use chrono::{DateTime, Utc};

/// Return the current UTC timestamp as a `DateTime<Utc>`.
pub fn now() -> DateTime<Utc> {
    Utc::now()
}

/// Format a duration in milliseconds for logging.
pub fn format_latency_ms(duration: std::time::Duration) -> i64 {
    duration.as_millis() as i64
}
