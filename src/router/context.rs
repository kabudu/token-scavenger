//! Authenticated routing context and reserved `x-ts-*` header parsing.
//!
//! Session and subtask values stay on this request. Logs and metrics receive
//! keyed digests only. A project API key is never accepted as a session id.

use crate::api::error::ApiError;
use crate::projects::ClientProjectContext;
use axum::http::HeaderMap;
use std::time::{Duration, Instant};

pub const MAX_ROUTING_HEADER_BYTES: usize = 1024;
const OPAQUE_ID_MAX: usize = 128;
const LABEL_MAX: usize = 64;

const ROUTING_HEADER_NAMES: &[&str] = &[
    "x-ts-session",
    "x-ts-subtask",
    "x-ts-task-type",
    "x-ts-tier",
    "x-ts-phase",
    "x-ts-affinity",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TierHint {
    Auto,
    Economy,
    Standard,
    Advanced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhaseHint {
    Auto,
    Planner,
    ToolResult,
    Finalize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AffinityHint {
    Off,
    Prefer,
    Required,
}

/// Validated client hints. Raw session ids are redacted in `Debug`.
pub struct RoutingHints {
    pub session: Option<String>,
    pub subtask: Option<String>,
    pub task_type: Option<String>,
    pub tier: Option<TierHint>,
    pub phase: Option<PhaseHint>,
    pub affinity: Option<AffinityHint>,
}

impl std::fmt::Debug for RoutingHints {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoutingHints")
            .field("session", &self.session.as_ref().map(|_| "[redacted]"))
            .field("subtask", &self.subtask.as_ref().map(|_| "[redacted]"))
            .field("task_type", &self.task_type)
            .field("tier", &self.tier)
            .field("phase", &self.phase)
            .field("affinity", &self.affinity)
            .finish()
    }
}

impl RoutingHints {
    pub fn empty() -> Self {
        Self {
            session: None,
            subtask: None,
            task_type: None,
            tier: None,
            phase: None,
            affinity: None,
        }
    }

    pub fn requests_affinity(&self) -> bool {
        self.session.is_some()
    }
}

/// Immutable per-request routing identity.
///
/// `public_request_id` becomes the unique effective storage and trace id after
/// the route handler claims it. A colliding client correlation id is replaced
/// with a generated UUID. Project context is captured from authentication.
pub struct RoutingContext {
    pub public_request_id: String,
    pub project: ClientProjectContext,
    pub hints: RoutingHints,
    pub deadline: Instant,
    /// False when the process is running without authentication. That mode is
    /// one shared trust domain (`principal_id = master`), not a per-client session.
    pub authenticated: bool,
    /// Preview-only simulated tier. Execution ignores this field.
    pub simulated_tier: Option<crate::config::schema::AgentTier>,
}

impl std::fmt::Debug for RoutingContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RoutingContext")
            .field("public_request_id", &self.public_request_id)
            .field("project_id", &self.project.project_id)
            .field("principal_id", &self.project.principal_id)
            .field("hints", &self.hints)
            .field("authenticated", &self.authenticated)
            .finish()
    }
}

pub fn routing_context(
    headers: &HeaderMap,
    project: ClientProjectContext,
    authenticated: bool,
    timeout: Duration,
) -> Result<RoutingContext, ApiError> {
    Ok(RoutingContext {
        public_request_id: public_request_id(headers),
        project,
        hints: parse_routing_headers(headers)?,
        deadline: Instant::now() + timeout,
        authenticated,
        simulated_tier: None,
    })
}

pub fn public_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && value.len() <= 200)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string())
}

pub fn parse_routing_headers(headers: &HeaderMap) -> Result<RoutingHints, ApiError> {
    let mut total_bytes = 0usize;
    for name in ROUTING_HEADER_NAMES {
        for value in headers.get_all(*name) {
            total_bytes = total_bytes.saturating_add(name.len());
            total_bytes = total_bytes.saturating_add(value.as_bytes().len());
        }
    }
    if total_bytes > MAX_ROUTING_HEADER_BYTES {
        return Err(invalid("routing headers exceed 1 KiB"));
    }

    let session = optional_opaque(headers, "x-ts-session", OPAQUE_ID_MAX)?;
    let subtask = optional_opaque(headers, "x-ts-subtask", OPAQUE_ID_MAX)?;
    let task_type = optional_label(headers, "x-ts-task-type")?;
    let tier = optional_enum(headers, "x-ts-tier", parse_tier)?;
    let phase = optional_enum(headers, "x-ts-phase", parse_phase)?;
    let affinity = optional_enum(headers, "x-ts-affinity", parse_affinity)?;

    if subtask.is_some() && session.is_none() {
        return Err(invalid("x-ts-subtask requires x-ts-session"));
    }

    Ok(RoutingHints {
        session,
        subtask,
        task_type,
        tier,
        phase,
        affinity,
    })
}

fn invalid(message: &str) -> ApiError {
    ApiError::InvalidRequest(message.to_string())
}

fn optional_opaque(
    headers: &HeaderMap,
    name: &str,
    max_len: usize,
) -> Result<Option<String>, ApiError> {
    let Some(value) = single_header(headers, name)? else {
        return Ok(None);
    };
    if !valid_opaque(&value, max_len) {
        return Err(invalid(&format!(
            "{name} must be 1–{max_len} ASCII characters from [A-Za-z0-9._:-]"
        )));
    }
    Ok(Some(value))
}

fn optional_label(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    let Some(value) = single_header(headers, name)? else {
        return Ok(None);
    };
    if !valid_opaque(&value, LABEL_MAX) {
        return Err(invalid(&format!(
            "{name} must be 1–{LABEL_MAX} ASCII characters from [A-Za-z0-9._:-]"
        )));
    }
    Ok(Some(value))
}

fn optional_enum<T>(
    headers: &HeaderMap,
    name: &str,
    parse: fn(&str) -> Option<T>,
) -> Result<Option<T>, ApiError> {
    let Some(value) = single_header(headers, name)? else {
        return Ok(None);
    };
    parse(&value)
        .map(Some)
        .ok_or_else(|| invalid(&format!("{name} is invalid")))
}

fn single_header(headers: &HeaderMap, name: &str) -> Result<Option<String>, ApiError> {
    let mut values = headers.get_all(name).iter();
    let Some(first) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() {
        return Err(invalid(&format!("duplicate {name} header")));
    }
    let text = first
        .to_str()
        .map_err(|_| invalid(&format!("{name} must be ASCII")))?;
    Ok(Some(text.trim().to_string()))
}

fn valid_opaque(value: &str, max_len: usize) -> bool {
    let len = value.len();
    (1..=max_len).contains(&len)
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b':' | b'-'))
}

fn parse_tier(value: &str) -> Option<TierHint> {
    match value {
        "auto" => Some(TierHint::Auto),
        "economy" => Some(TierHint::Economy),
        "standard" => Some(TierHint::Standard),
        "advanced" => Some(TierHint::Advanced),
        _ => None,
    }
}

fn parse_phase(value: &str) -> Option<PhaseHint> {
    match value {
        "auto" => Some(PhaseHint::Auto),
        "planner" => Some(PhaseHint::Planner),
        "tool_result" => Some(PhaseHint::ToolResult),
        "finalize" => Some(PhaseHint::Finalize),
        _ => None,
    }
}

fn parse_affinity(value: &str) -> Option<AffinityHint> {
    match value {
        "off" => Some(AffinityHint::Off),
        "prefer" => Some(AffinityHint::Prefer),
        "required" => Some(AffinityHint::Required),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn rejects_duplicate_and_subtask_without_session() {
        let mut headers = HeaderMap::new();
        headers.append("x-ts-session", HeaderValue::from_static("run-1"));
        headers.append("x-ts-session", HeaderValue::from_static("run-2"));
        assert!(parse_routing_headers(&headers).is_err());

        let mut headers = HeaderMap::new();
        headers.insert("x-ts-subtask", HeaderValue::from_static("extract"));
        assert!(parse_routing_headers(&headers).is_err());
    }

    #[test]
    fn accepts_a_scoped_hint_set() {
        let mut headers = HeaderMap::new();
        headers.insert("x-ts-session", HeaderValue::from_static("run-1"));
        headers.insert(
            "x-ts-subtask",
            HeaderValue::from_static("extract-citations"),
        );
        headers.insert("x-ts-task-type", HeaderValue::from_static("extract"));
        headers.insert("x-ts-tier", HeaderValue::from_static("auto"));
        let hints = parse_routing_headers(&headers).unwrap();
        assert_eq!(hints.session.as_deref(), Some("run-1"));
        assert_eq!(hints.task_type.as_deref(), Some("extract"));
        assert!(format!("{hints:?}").contains("[redacted]"));
        assert!(!format!("{hints:?}").contains("run-1"));
    }
}
