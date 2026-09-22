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
    app_with_profiles_at(agent, &base_url).await
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

async fn decision_value(state: &AppState, request_id: &str) -> serde_json::Value {
    let (details,): (String,) = sqlx::query_as(
        "SELECT details_json FROM request_trace_events
         WHERE request_id = ? AND event_type = 'agent_decision'",
    )
    .bind(request_id)
    .fetch_one(&state.db)
    .await
    .unwrap();
    serde_json::from_str(&details).unwrap()
}

#[tokio::test]
async fn classifier_calls_the_provider_once_and_then_uses_the_cache() {
    let state = app_with_profiles(classifier_agent()).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());
    for id in ["class-1", "class-2"] {
        let response = app
            .clone()
            .oneshot(chat("agent-auto", "lookup", id))
            .await
            .unwrap();
        let status = response.status();
        let bytes = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "{}",
            String::from_utf8_lossy(&bytes)
        );
    }
    let first = decision_value(&state, "req-class-1").await;
    let second = decision_value(&state, "req-class-2").await;
    assert_eq!(first["tier"], "economy");
    assert_eq!(first["classifier_status"], "classified");
    assert_eq!(first["classifier_cache"], "miss");
    assert_eq!(second["tier"], "economy");
    assert_eq!(second["classifier_status"], "classifier_cache_hit");
    assert_eq!(second["classifier_cache"], "hit");
    let calls: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM usage_events WHERE purpose = 'classification'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(calls.0, 1);
}

fn classifier_agent() -> AgentRoutingConfig {
    let mut agent = rules_agent();
    agent.mode = AgentRoutingMode::Adaptive;
    agent.rules.clear();
    agent.classifier.enabled = true;
    agent.classifier.provider_id = "groq".into();
    agent.classifier.model_id = "classifier-model".into();
    agent.classifier.allowed_project_ids = vec!["default".into()];
    agent.classifier.confidence_threshold = 0.5;
    agent
}

#[tokio::test]
async fn completed_stream_pins_the_subtask_and_a_dropped_stream_does_not() {
    let hold = std::sync::Arc::new(tokio::sync::Notify::new());
    let mock = common::MockProviderState {
        hold_stream_after_first_chunk: Some(hold.clone()),
        ..Default::default()
    };
    let (base_url, _server) = common::start_mock_server(mock).await;
    let mut agent = rules_agent();
    agent.profiles.get_mut("agent-auto").unwrap().affinity = AffinityMode::Required;
    let state = app_with_profiles_at(agent, &base_url).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());

    let response = app
        .clone()
        .oneshot(stream_chat("stream-pin"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _ = tokio::time::timeout(
        std::time::Duration::from_millis(400),
        axum::body::to_bytes(response.into_body(), 1024),
    )
    .await;
    hold.notify_one();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let mut continued_body = String::new();
    let mut continued_status = StatusCode::OK;
    for attempt in 0..40 {
        let continued = app
            .clone()
            .oneshot(tool_continuation("stream-pin", attempt))
            .await
            .unwrap();
        continued_status = continued.status();
        let bytes = axum::body::to_bytes(continued.into_body(), 65536)
            .await
            .unwrap();
        continued_body = String::from_utf8_lossy(&bytes).into_owned();
        if continued_status == StatusCode::CONFLICT && continued_body.contains("subtask_busy") {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            continue;
        }
        break;
    }
    assert_eq!(continued_status, StatusCode::CONFLICT, "{continued_body}");
    let json: serde_json::Value = serde_json::from_str(&continued_body).unwrap();
    assert_eq!(json["error"]["code"], "session_state_unavailable");

    let complete = common::MockProviderState::default();
    let (base_url, _server) = common::start_mock_server(complete).await;
    let mut agent = rules_agent();
    agent.profiles.get_mut("agent-auto").unwrap().affinity = AffinityMode::Prefer;
    let state = app_with_profiles_at(agent, &base_url).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());
    let response = app.clone().oneshot(stream_chat("stream-ok")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let _ = axum::body::to_bytes(response.into_body(), 65536)
        .await
        .unwrap();
    let mut follow_id = String::new();
    let mut follow_status = StatusCode::CONFLICT;
    let mut follow_body = String::new();
    for attempt in 0..40 {
        follow_id = format!("req-stream-follow-{attempt}");
        let follow = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/v1/chat/completions")
                    .header("content-type", "application/json")
                    .header("x-request-id", &follow_id)
                    .header("x-ts-session", "stream-run")
                    .header("x-ts-subtask", "stream-ok")
                    .header("x-ts-task-type", "extract")
                    .body(Body::from(
                        serde_json::json!({
                            "model": "agent-auto",
                            "messages": [{"role": "user", "content": "extract"}]
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        follow_status = follow.status();
        let bytes = axum::body::to_bytes(follow.into_body(), 65536)
            .await
            .unwrap();
        follow_body = String::from_utf8_lossy(&bytes).into_owned();
        if follow_status == StatusCode::CONFLICT && follow_body.contains("subtask_busy") {
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
            continue;
        }
        break;
    }
    assert_eq!(follow_status, StatusCode::OK, "{follow_body}");
    let decision = decision_value(&state, &follow_id).await;
    assert_eq!(decision["pin"], "pin_hit");
}

fn stream_chat(subtask: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-request-id", format!("req-{subtask}"))
        .header("x-ts-session", "stream-run")
        .header("x-ts-subtask", subtask)
        .header("x-ts-affinity", "required")
        .body(Body::from(
            serde_json::json!({
                "model": "agent-auto",
                "stream": true,
                "messages": [{"role": "user", "content": "write the draft"}]
            })
            .to_string(),
        ))
        .unwrap()
}

fn tool_continuation(subtask: &str, attempt: usize) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/v1/chat/completions")
        .header("content-type", "application/json")
        .header("x-request-id", format!("req-{subtask}-next-{attempt}"))
        .header("x-ts-session", "stream-run")
        .header("x-ts-subtask", subtask)
        .header("x-ts-affinity", "required")
        .body(Body::from(
            serde_json::json!({
                "model": "agent-auto",
                "messages": [
                    {"role": "assistant", "content": "", "tool_calls": [{
                        "id": "a",
                        "type": "function",
                        "function": {"name": "lookup", "arguments": "{}"}
                    }]},
                    {"role": "tool", "tool_call_id": "a", "content": "one"}
                ]
            })
            .to_string(),
        ))
        .unwrap()
}

async fn app_with_profiles_at(agent: AgentRoutingConfig, base_url: &str) -> AppState {
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
    seed_groq(&state).await;
    state
}

async fn seed_groq(state: &AppState) {
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
}

#[tokio::test]
async fn admin_project_key_can_call_a_profile() {
    let mock = common::MockProviderState::default();
    let (base_url, _server) = common::start_mock_server(mock).await;
    let pool = sqlx::SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    let mut config = Config::default();
    config.server.master_api_key = "master-secret".into();
    config.routing.provider_order = vec!["groq".into()];
    config.routing.agent = rules_agent();
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
    seed_groq(&state).await;
    let app = tokenscavenger::app::startup::build_router(state.clone());

    let created = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/projects")
                .header("authorization", "Bearer master-secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "project_id": "desk",
                        "display_name": "Desk",
                        "allowed_model_groups": ["agent-auto"]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(created.status(), StatusCode::OK);
    let issued = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin/projects/desk/keys")
                .header("authorization", "Bearer master-secret")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({"label": "laptop"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(issued.status(), StatusCode::OK);
    let issued_body = axum::body::to_bytes(issued.into_body(), 65536)
        .await
        .unwrap();
    let issued_json: serde_json::Value = serde_json::from_slice(&issued_body).unwrap();
    let api_key = issued_json["api_key"].as_str().unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/v1/chat/completions")
                .header("authorization", format!("Bearer {api_key}"))
                .header("content-type", "application/json")
                .header("x-request-id", "desk-1")
                .header("x-ts-session", "desk-run")
                .header("x-ts-subtask", "plan")
                .header("x-ts-task-type", "plan")
                .body(Body::from(
                    serde_json::json!({
                        "model": "agent-auto",
                        "messages": [{"role": "user", "content": "plan the chapter"}]
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let project: (String,) =
        sqlx::query_as("SELECT project_id FROM request_log WHERE request_id = 'desk-1'")
            .fetch_one(&state.db)
            .await
            .unwrap();
    assert_eq!(project.0, "desk");
    let decision = decision_value(&state, "desk-1").await;
    assert_eq!(decision["tier"], "advanced");
}
