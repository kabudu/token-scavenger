//! Tool-continuation portability.
//!
//! Pinning a provider and model does not preserve provider-specific opaque
//! continuation fields. Google Gemini function calls can require thought
//! signatures that this proxy does not round-trip. Those continuations are
//! rejected instead of being advertised as supported.
//!
//! OpenAI-shaped histories that the adapters replay through
//! `messages` / `tool_calls` / `tool_call_id` are treated as replayable.
//! See `documentation/provider-matrix.md`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContinuationSupport {
    /// Normalised OpenAI tool history can be replayed to this provider.
    Replayable,
    /// The adapter needs opaque continuation state that is not stored.
    OpaqueRequired,
}

pub fn continuation_support(provider_id: &str) -> ContinuationSupport {
    match provider_id {
        // Gemini generateContent function-call turns need thought signatures.
        "google" => ContinuationSupport::OpaqueRequired,
        _ => ContinuationSupport::Replayable,
    }
}

pub fn is_replayable(provider_id: &str) -> bool {
    continuation_support(provider_id) == ContinuationSupport::Replayable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_tool_continuations_are_not_replayable() {
        assert_eq!(
            continuation_support("google"),
            ContinuationSupport::OpaqueRequired
        );
        assert!(is_replayable("groq"));
        assert!(is_replayable("openrouter"));
        assert!(is_replayable("local"));
    }
}
