//! Opt-in subtask routing over the public chat endpoint.

mod common;

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tokenscavenger::app::state::AppState;
use tokenscavenger::config::schema::{
    AffinityMode, AgentClassifierConfig, AgentProfileConfig, AgentRoutingConfig, AgentRoutingMode,
    AgentRuleConfig, AgentTier, Config, ProviderConfig,
};
use tower::ServiceExt;

async fn app_with_profiles(agent: AgentRoutingConfig) -> AppState {
    let mock = common::MockProviderState {
        status_code: 500,
        usage_tokens: (12, 8),
        ..Default::default()
    };
    let (base_url, _handle) = common::start_mock_server(mock).await;
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut config = Config::default();
    config.server.master_api_key = String::new();
    config.routing.provider_order = vec!["groq".into()];
    config.routing.agent = agent;
    config.providers = vec![ProviderConfig {
        id: "groq".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: Some("test-key".into()),
        free_only: true,
        discover_models: false,
        embedding_support: Default::default(),
    }];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;
    sqlx::query(
        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
         VALUES ('groq', 'Groq', 1, 'http://127.0.0.1', 1)",
    )
    .execute(&state.db)
    .await
    .unwrap();
    for model in ["cheap-model", "smart-model", "test-model"] {
        sqlx::query(
            "INSERT INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, supports_tools, discovered_at, updated_at)
             VALUES ('groq', ?, ?, 1, 1, 1, 1, datetime('now'), datetime('now'))",
        )
        .bind(model)
        .bind(model)
        .execute(&state.db)
        .await
        .unwrap();
    }
    for (name, target) in [
        ("economy", "cheap-model"),
        ("standard", "cheap-model"),
        ("advanced", "smart-model"),
    ] {
        sqlx::query("INSERT INTO model_groups (name, target_json, enabled) VALUES (?, ?, 1)")
            .bind(name)
            .bind(format!("[\"{target}\"]"))
            .execute(&state.db)
            .await
            .unwrap();
    }
    state
}

fn rules_agent() -> AgentRoutingConfig {
    AgentRoutingConfig {
        enabled: true,
        mode: AgentRoutingMode::Rules,
        profiles: [(
            "agent-auto".into(),
            AgentProfileConfig {
                default_tier: AgentTier::Standard,
                economy_group: "economy".into(),
                standard_group: "standard".into(),
                advanced_group: "advanced".into(),
                affinity: AffinityMode::Prefer,
            },
        )]
        .into_iter()
        .collect(),
        rules: vec![
            rule("plan", AgentTier::Advanced),
            rule("extract", AgentTier::Economy),
            rule("format", AgentTier::Economy),
            rule("verify", AgentTier::Economy),
        ],
        classifier: AgentClassifierConfig::default(),
        ..AgentRoutingConfig::default()
    }
}

fn rule(task: &str, tier: AgentTier) -> AgentRuleConfig {
    AgentRuleConfig {
        profile: "agent-auto".into(),
        task_type: Some(task.into()),
        phase: None,
        tools_required: None,
        json_required: None,
        vision_required: None,
        min_input_bytes: None,
        max_input_bytes: None,
        tier,
    }
}

fn chat(model: &str, task: &str, subtask: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-request-id", format!("req-{subtask}"))
        .header("x-ts-session", "run-1")
        .header("x-ts-subtask", subtask)
        .header("x-ts-task-type", task)
        .body(Body::from(
            serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": task}]
            })
            .to_string(),
        ))
        .unwrap()
}

#[test]
fn rules_corpus_beats_always_default_without_claiming_quality() {
    let corpus: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/agent_routing_eval.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    let advanced = cases
        .iter()
        .filter(|case| case["rules_tier"] == "advanced")
        .count();
    let economy = cases
        .iter()
        .filter(|case| case["rules_tier"] == "economy")
        .count();
    assert_eq!(advanced, 1);
    assert_eq!(economy, 3);
    assert!(cases.iter().all(|case| case["default_tier"] == "standard"));
}

#[tokio::test]
async fn four_subtasks_keep_one_advanced_and_three_economy_routes() {
    let state = app_with_profiles(rules_agent()).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());
    for (task, subtask, expected) in [
        ("plan", "plan", "smart-model"),
        ("extract", "extract", "cheap-model"),
        ("format", "format", "cheap-model"),
        ("verify", "verify", "cheap-model"),
    ] {
        let response = app
            .clone()
            .oneshot(chat("agent-auto", task, subtask))
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{task} {}",
            String::from_utf8_lossy(&bytes)
        );
        let selected: (String,) = sqlx::query_as(
            "SELECT model_id FROM request_trace_events
             WHERE request_id = ? AND event_type = 'attempt_started'",
        )
        .bind(format!("req-{subtask}"))
        .fetch_one(&state.db)
        .await
        .unwrap();
        assert_eq!(selected.0, expected, "{task}");
        let tier: (String,) = sqlx::query_as("SELECT tier FROM request_log WHERE request_id = ?")
            .bind(format!("req-{subtask}"))
            .fetch_one(&state.db)
            .await
            .unwrap();
        assert_eq!(
            tier.0,
            if expected == "smart-model" {
                "advanced"
            } else {
                "economy"
            }
        );
    }
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_log")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(rows, 4);
}

#[tokio::test]
async fn disabled_feature_ignores_valid_hints_and_rejects_malformed_ones() {
    let state = app_with_profiles(AgentRoutingConfig::default()).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());
    let ok = app
        .clone()
        .oneshot(chat("test-model", "plan", "plan"))
        .await
        .unwrap();
    let status = ok.status();
    let bytes = axum::body::to_bytes(ok.into_body(), 65536).await.unwrap();
    assert_eq!(
        status,
        StatusCode::OK,
        "{}",
        String::from_utf8_lossy(&bytes)
    );
    let selected: (String,) =
        sqlx::query_as("SELECT selected_model_id FROM request_log WHERE request_id = 'req-plan'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(selected.0, "test-model");

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .header("x-ts-subtask", "extract")
                .body(Body::from(
                    serde_json::json!({
                        "model": "test-model",
                        "messages": [{"role": "user", "content": "hi"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn required_tool_continuation_without_a_pin_is_a_conflict() {
    let mut agent = rules_agent();
    agent.profiles.get_mut("agent-auto").unwrap().affinity = AffinityMode::Required;
    let state = app_with_profiles(agent).await;
    let app = tokenscavenger::app::startup::build_router(state);
    let body = serde_json::json!({
        "model": "agent-auto",
        "messages": [
            {"role": "user", "content": "look this up"},
            {"role": "assistant", "content": "", "tool_calls": [{"id": "call-1", "type": "function", "function": {"name": "lookup", "arguments": "{}"}}]},
            {"role": "tool", "tool_call_id": "call-1", "content": "result"}
        ]
    });
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .header("x-ts-session", "run-1")
                .header("x-ts-subtask", "lookup")
                .header("x-ts-affinity", "required")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["error"]["code"], "session_state_unavailable");
}

#[tokio::test]
async fn preview_does_not_create_a_request_row() {
    let state = app_with_profiles(rules_agent()).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/route-plan/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "model": "agent-auto",
                        "task_type": "extract",
                        "messages": [{"role": "user", "content": "pull the citations"}],
                        "simulated_tier": "advanced"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(json["decision"]["simulated"], true);
    assert_eq!(json["decision"]["tier"], "advanced");
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM request_log")
        .fetch_one(&state.db)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[tokio::test]
async fn concurrent_paid_calls_cannot_both_pass_one_daily_ceiling() {
    let mock = common::MockProviderState {
        usage_tokens: (0, 1000),
        ..Default::default()
    };
    let (base_url, _handle) = common::start_mock_server(mock).await;
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut agent = rules_agent();
    agent.profiles.get_mut("agent-auto").unwrap().affinity = AffinityMode::Off;
    let mut config = Config::default();
    config.server.master_api_key = String::new();
    config.routing.allow_paid_fallback = true;
    config.routing.provider_order = vec!["groq".into()];
    config.routing.budgets.max_cost_per_day_usd = Some(1_500.0);
    config.routing.agent = agent;
    config.providers = vec![ProviderConfig {
        id: "groq".into(),
        enabled: true,
        base_url: Some(format!("{base_url}/v1")),
        api_key: Some("test-key".into()),
        free_only: false,
        discover_models: false,
        embedding_support: Default::default(),
    }];
    let state = AppState::new(
        config,
        pool,
        Default::default(),
        tokio::sync::broadcast::channel(1).0,
    );
    state.provider_registry.init_from_config(&state).await;
    sqlx::query(
        "INSERT INTO providers (provider_id, display_name, enabled, base_url, free_only)
         VALUES ('groq', 'Groq', 1, 'http://127.0.0.1', 0)",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO models (provider_id, upstream_model_id, public_model_id, enabled, free_tier, supports_chat, supports_tools, discovered_at, updated_at)
         VALUES ('groq', 'cheap-model', 'cheap-model', 1, 0, 1, 1, datetime('now'), datetime('now'))",
    )
    .execute(&state.db)
    .await
    .unwrap();
    sqlx::query("INSERT INTO model_groups (name, target_json, enabled) VALUES ('standard', '[\"cheap-model\"]', 1)")
        .execute(&state.db)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO model_pricing
         (provider_id, model_id, input_per_1m, output_per_1m, source_kind, confidence)
         VALUES ('groq', 'cheap-model', 0, 1000000, 'operator_override', 'provider_published')",
    )
    .execute(&state.db)
    .await
    .unwrap();

    let app = tokenscavenger::app::startup::build_router(state);
    let send = |id: &str| {
        app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("content-type", "application/json")
                .header("x-request-id", id)
                .body(Body::from(
                    serde_json::json!({
                        "model": "agent-auto",
                        "max_tokens": 1000,
                        "messages": [{"role": "user", "content": "budget"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
    };
    let (left, right) = tokio::join!(send("paid-a"), send("paid-b"));
    let mut statuses = Vec::new();
    for response in [left.unwrap(), right.unwrap()] {
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        statuses.push((status, String::from_utf8_lossy(&bytes).to_string()));
    }
    let succeeded = statuses
        .iter()
        .filter(|(status, _)| *status == StatusCode::OK)
        .count();
    assert_eq!(succeeded, 1, "{statuses:?}");
    assert!(
        statuses.iter().any(|(status, body)| {
            *status == StatusCode::TOO_MANY_REQUESTS && body.contains("budget_denied")
                || *status == StatusCode::SERVICE_UNAVAILABLE
        }),
        "{statuses:?}"
    );
}
