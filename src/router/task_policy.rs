//! Phase detection and versioned tier rules.
//!
//! Rules are data. Tool declarations are a capability requirement. A
//! continuation is a matched assistant tool-call followed by tool results.
//! Ambiguous histories are not repaired.

use crate::api::openai::chat::{ChatMessage, NormalizedChatRequest};
use crate::config::schema::{AgentProfileConfig, AgentRuleConfig, AgentTier};
use crate::discovery::model_intelligence::ModelRequestRequirements;
use crate::router::context::{PhaseHint, TierHint};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Initial,
    ToolResult,
    Finalize,
    Ambiguous,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Initial => "initial",
            Self::ToolResult => "tool_result",
            Self::Finalize => "finalize",
            Self::Ambiguous => "ambiguous",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhaseReport {
    pub phase: Phase,
    pub tools_required: bool,
    pub json_required: bool,
    pub vision_required: bool,
    pub input_bytes: usize,
    pub continuation: bool,
    pub partial_tool_results: bool,
    pub orphan_tool_ids: bool,
    pub duplicate_tool_ids: bool,
    pub header_overridden: bool,
    pub matched_tool_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TierSource {
    Hint,
    Rule { index: usize },
    Classified,
    ClassifierTimeout,
    ClassifierSaturated,
    ClassifierInvalid,
    ClassifierLowConfidence,
    ClassifierSkipped,
    Default,
    Continuation,
    Simulated,
}

impl TierSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Hint => "hint",
            Self::Rule { .. } => "rule",
            Self::Classified => "classified",
            Self::ClassifierTimeout => "classifier_timeout",
            Self::ClassifierSaturated => "classifier_saturated",
            Self::ClassifierInvalid => "classifier_invalid",
            Self::ClassifierLowConfidence => "classifier_low_confidence",
            Self::ClassifierSkipped => "classifier_skipped",
            Self::Default => "default",
            Self::Continuation => "continuation_required",
            Self::Simulated => "simulated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TierDecision {
    pub tier: AgentTier,
    pub source: TierSource,
    pub rule_index: Option<usize>,
    pub confidence: Option<String>,
}

pub fn detect_phase(request: &NormalizedChatRequest, header: Option<PhaseHint>) -> PhaseReport {
    let requirements = ModelRequestRequirements::for_chat(request);
    let structural = structural_phase(&request.messages);
    let mut report = PhaseReport {
        phase: structural.phase,
        tools_required: requirements.requires_tools,
        json_required: requirements.requires_json_mode,
        vision_required: requirements.requires_vision,
        input_bytes: request.prompt_size_hint(),
        continuation: structural.phase == Phase::ToolResult,
        partial_tool_results: structural.partial,
        orphan_tool_ids: structural.orphan,
        duplicate_tool_ids: structural.duplicate,
        header_overridden: false,
        matched_tool_ids: structural.matched_ids,
    };

    if structural.phase == Phase::Ambiguous {
        report.header_overridden = header.is_some_and(|hint| hint != PhaseHint::Auto);
        return report;
    }

    match header {
        Some(PhaseHint::ToolResult) if structural.phase != Phase::ToolResult => {
            report.header_overridden = true;
        }
        Some(PhaseHint::Finalize) if structural.phase == Phase::ToolResult => {
            report.header_overridden = true;
        }
        Some(PhaseHint::Finalize) if structural.phase != Phase::ToolResult => {
            report.phase = Phase::Finalize;
            report.continuation = false;
        }
        Some(PhaseHint::Planner) if structural.phase == Phase::Initial => {
            report.phase = Phase::Initial;
        }
        _ => {}
    }
    report
}

struct StructuralPhase {
    phase: Phase,
    partial: bool,
    orphan: bool,
    duplicate: bool,
    matched_ids: Vec<String>,
}

fn structural_phase(messages: &[ChatMessage]) -> StructuralPhase {
    let mut expected = std::collections::HashSet::new();
    let mut received = std::collections::HashSet::new();
    let mut round_ids = Vec::new();
    let mut duplicate = false;
    let mut orphan = false;
    let mut partial = false;
    let mut ends_with_results = false;
    for message in messages {
        if message.role == "assistant"
            && message
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
        {
            if !expected.is_empty() && received.len() != expected.len() {
                partial = true;
            }
            expected.clear();
            received.clear();
            round_ids.clear();
            for call in message.tool_calls.as_ref().unwrap() {
                if call.id.is_empty() || !expected.insert(call.id.as_str()) {
                    duplicate = true;
                }
                round_ids.push(call.id.clone());
            }
            ends_with_results = false;
        } else if message.role == "tool" {
            match message.tool_call_id.as_deref() {
                Some(id) if expected.contains(id) => {
                    if !received.insert(id) {
                        duplicate = true;
                    }
                    ends_with_results = received.len() == expected.len();
                }
                _ => orphan = true,
            }
        } else {
            if !expected.is_empty() && received.len() != expected.len() {
                partial = true;
            }
            expected.clear();
            received.clear();
            ends_with_results = false;
        }
    }
    if !expected.is_empty() && received.len() != expected.len() {
        partial = true;
    }

    let phase = if duplicate || orphan || partial {
        Phase::Ambiguous
    } else if ends_with_results {
        Phase::ToolResult
    } else {
        Phase::Initial
    };

    StructuralPhase {
        phase,
        partial,
        orphan,
        duplicate,
        matched_ids: if ends_with_results {
            round_ids
        } else {
            Vec::new()
        },
    }
}

#[allow(clippy::too_many_arguments)]
pub fn select_tier_without_classifier(
    profile: &AgentProfileConfig,
    rules: &[AgentRuleConfig],
    profile_name: &str,
    report: &PhaseReport,
    task_type: Option<&str>,
    tier_hint: Option<TierHint>,
    continuation_tier: Option<AgentTier>,
    simulated: Option<AgentTier>,
) -> TierDecision {
    if let Some(tier) = continuation_tier {
        return TierDecision {
            tier,
            source: TierSource::Continuation,
            rule_index: None,
            confidence: None,
        };
    }
    if let Some(tier) = simulated {
        return TierDecision {
            tier,
            source: TierSource::Simulated,
            rule_index: None,
            confidence: Some("simulation".into()),
        };
    }
    if let Some(tier) = explicit_hint(tier_hint) {
        return TierDecision {
            tier,
            source: TierSource::Hint,
            rule_index: None,
            confidence: None,
        };
    }
    if let Some((index, rule)) = matching_rule(rules, profile_name, report, task_type) {
        return TierDecision {
            tier: rule.tier,
            source: TierSource::Rule { index },
            rule_index: Some(index),
            confidence: None,
        };
    }
    TierDecision {
        tier: profile.default_tier,
        source: TierSource::Default,
        rule_index: None,
        confidence: None,
    }
}

pub fn explicit_hint(hint: Option<TierHint>) -> Option<AgentTier> {
    match hint {
        Some(TierHint::Economy) => Some(AgentTier::Economy),
        Some(TierHint::Standard) => Some(AgentTier::Standard),
        Some(TierHint::Advanced) => Some(AgentTier::Advanced),
        _ => None,
    }
}

pub fn matching_rule<'a>(
    rules: &'a [AgentRuleConfig],
    profile_name: &str,
    report: &PhaseReport,
    task_type: Option<&str>,
) -> Option<(usize, &'a AgentRuleConfig)> {
    rules.iter().enumerate().find(|(_, rule)| {
        rule.profile == profile_name
            && rule
                .task_type
                .as_ref()
                .is_none_or(|expected| task_type == Some(expected.as_str()))
            && rule
                .phase
                .as_ref()
                .is_none_or(|phase| phase_matches(phase, report.phase))
            && rule
                .tools_required
                .is_none_or(|required| required == report.tools_required)
            && rule
                .json_required
                .is_none_or(|required| required == report.json_required)
            && rule
                .vision_required
                .is_none_or(|required| required == report.vision_required)
            && rule
                .min_input_bytes
                .is_none_or(|min| report.input_bytes as u64 >= min)
            && rule
                .max_input_bytes
                .is_none_or(|max| (report.input_bytes as u64) <= max)
    })
}

fn phase_matches(configured: &str, phase: Phase) -> bool {
    match configured {
        "auto" => true,
        "planner" => phase == Phase::Initial,
        "initial" => phase == Phase::Initial,
        "tool_result" => phase == Phase::ToolResult,
        "finalize" => phase == Phase::Finalize,
        _ => false,
    }
}

pub fn policy_revision(agent_json: &str) -> String {
    let digest = Sha256::digest(agent_json.as_bytes());
    hex_prefix(&digest)
}

fn hex_prefix(bytes: &[u8]) -> String {
    bytes
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn rule_would_resolve(
    rules: &[AgentRuleConfig],
    profile_name: &str,
    report: &PhaseReport,
    task_type: Option<&str>,
    tier_hint: Option<TierHint>,
    continuation_tier: Option<AgentTier>,
) -> bool {
    continuation_tier.is_some()
        || explicit_hint(tier_hint).is_some()
        || matching_rule(rules, profile_name, report, task_type).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::openai::chat::{ChatMessage, ToolCall, ToolCallFunction};

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: Some(serde_json::Value::String(content.into())),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }
    }

    fn tool_call(id: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            call_type: "function".into(),
            function: ToolCallFunction {
                name: "lookup".into(),
                arguments: "{}".into(),
            },
        }
    }

    fn request(messages: Vec<ChatMessage>) -> NormalizedChatRequest {
        NormalizedChatRequest {
            model: "agent-auto".into(),
            messages,
            temperature: None,
            top_p: None,
            max_tokens: None,
            stream: false,
            stop: None,
            presence_penalty: None,
            frequency_penalty: None,
            user: None,
            response_format: None,
            tools: None,
            tool_choice: None,
        }
    }

    #[test]
    fn matches_parallel_tool_results_and_flags_partial_sets() {
        let mut assistant = message("assistant", "");
        assistant.tool_calls = Some(vec![tool_call("a"), tool_call("b")]);
        let mut tool_a = message("tool", "one");
        tool_a.tool_call_id = Some("a".into());
        let mut tool_b = message("tool", "two");
        tool_b.tool_call_id = Some("b".into());
        let report = detect_phase(&request(vec![assistant.clone(), tool_a, tool_b]), None);
        assert_eq!(report.phase, Phase::ToolResult);
        assert!(report.continuation);

        let mut only_a = message("tool", "one");
        only_a.tool_call_id = Some("a".into());
        let partial = detect_phase(
            &request(vec![assistant, only_a]),
            Some(PhaseHint::ToolResult),
        );
        assert_eq!(partial.phase, Phase::Ambiguous);
        assert!(partial.partial_tool_results);
        assert!(partial.header_overridden);
    }

    #[test]
    fn completed_prior_tool_round_is_not_an_orphan() {
        let mut first_call = message("assistant", "");
        first_call.tool_calls = Some(vec![tool_call("a")]);
        let mut first_result = message("tool", "first");
        first_result.tool_call_id = Some("a".into());
        let mut second_call = message("assistant", "");
        second_call.tool_calls = Some(vec![tool_call("b")]);
        let mut second_result = message("tool", "second");
        second_result.tool_call_id = Some("b".into());

        let history = vec![
            message("user", "look up both"),
            first_call,
            first_result,
            second_call.clone(),
            second_result,
        ];
        let report = detect_phase(&request(history), None);
        assert_eq!(report.phase, Phase::ToolResult);
        assert_eq!(report.matched_tool_ids, vec!["b"]);
        assert!(!report.orphan_tool_ids);

        let missing = detect_phase(&request(vec![second_call]), None);
        assert_eq!(missing.phase, Phase::Ambiguous);
        assert!(missing.partial_tool_results);
    }

    #[test]
    fn tools_alone_are_not_a_continuation() {
        let mut req = request(vec![message("user", "plan this")]);
        req.tools = Some(vec![crate::api::openai::chat::ToolDefinition {
            tool_type: "function".into(),
            function: crate::api::openai::chat::ToolFunction {
                name: "lookup".into(),
                description: None,
                parameters: None,
            },
        }]);
        let report = detect_phase(&req, Some(PhaseHint::ToolResult));
        assert_eq!(report.phase, Phase::Initial);
        assert!(report.tools_required);
        assert!(report.header_overridden);
    }

    #[test]
    fn unknown_task_uses_the_default_tier() {
        let profile = AgentProfileConfig {
            default_tier: AgentTier::Standard,
            economy_group: "e".into(),
            standard_group: "s".into(),
            advanced_group: "a".into(),
            affinity: Default::default(),
        };
        let rules = vec![AgentRuleConfig {
            profile: "agent-auto".into(),
            task_type: Some("extract".into()),
            phase: None,
            tools_required: None,
            json_required: None,
            vision_required: None,
            min_input_bytes: None,
            max_input_bytes: None,
            tier: AgentTier::Economy,
        }];
        let report = detect_phase(&request(vec![message("user", "do something new")]), None);
        let decision = select_tier_without_classifier(
            &profile,
            &rules,
            "agent-auto",
            &report,
            Some("brand-new"),
            None,
            None,
            None,
        );
        assert_eq!(decision.tier, AgentTier::Standard);
        assert_eq!(decision.source, TierSource::Default);
    }

    #[test]
    fn two_hundred_concurrent_rule_decisions_stay_under_two_milliseconds() {
        use std::sync::{Arc, Barrier};
        use std::thread;
        use std::time::{Duration, Instant};

        let profile = AgentProfileConfig {
            default_tier: AgentTier::Standard,
            economy_group: "e".into(),
            standard_group: "s".into(),
            advanced_group: "a".into(),
            affinity: Default::default(),
        };
        let rules = vec![AgentRuleConfig {
            profile: "agent-auto".into(),
            task_type: Some("extract".into()),
            phase: None,
            tools_required: None,
            json_required: None,
            vision_required: None,
            min_input_bytes: None,
            max_input_bytes: None,
            tier: AgentTier::Economy,
        }];
        let request = request(vec![message("user", "extract the citations")]);
        let gate = Arc::new(Barrier::new(200));
        let mut handles = Vec::with_capacity(200);
        for _ in 0..200 {
            let gate = Arc::clone(&gate);
            let profile = profile.clone();
            let rules = rules.clone();
            let request = request.clone();
            handles.push(thread::spawn(move || {
                gate.wait();
                let started = Instant::now();
                let report = detect_phase(&request, None);
                let decision = select_tier_without_classifier(
                    &profile,
                    &rules,
                    "agent-auto",
                    &report,
                    Some("extract"),
                    None,
                    None,
                    None,
                );
                assert_eq!(decision.tier, AgentTier::Economy);
                started.elapsed()
            }));
        }
        let mut samples: Vec<Duration> = handles
            .into_iter()
            .map(|handle| handle.join().expect("decision thread"))
            .collect();
        samples.sort();
        let p95 = samples[189];
        let p99 = samples[197];
        assert!(p95 <= Duration::from_millis(2), "p95 {p95:?} p99 {p99:?}");
        assert!(p99 <= Duration::from_millis(5), "p95 {p95:?} p99 {p99:?}");
    }
}
