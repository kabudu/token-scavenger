//! Database migration tests.
//!
//! Verifies that migrations work correctly:
//! - Clean bootstrap creates all expected tables
//! - Running migrations on an already-migrated database is idempotent

use sqlx::SqlitePool;

#[tokio::test]
async fn test_clean_bootstrap() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    // Verify all tables exist
    let tables = sqlx::query_as::<_, (String,)>(
        "SELECT name FROM sqlite_master WHERE type='table' ORDER BY name",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let table_names: Vec<String> = tables.into_iter().map(|r| r.0).collect();

    let expected = [
        "aliases",
        "config_audit_log",
        "config_snapshots",
        "discovery_runs",
        "models",
        "provider_health_events",
        "providers",
        "request_log",
        "usage_events",
        "_sqlx_migrations",
    ];

    for table in &expected {
        assert!(
            table_names.contains(&table.to_string()),
            "Missing table: {}",
            table
        );
    }
}

#[tokio::test]
async fn test_idempotent_migrations() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();
    // Run again — should be idempotent
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(count.0 > 0, "Migrations table should have entries");
}

#[tokio::test]
async fn test_can_insert_and_query_all_tables() {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("src/db/migrations")
        .run(&pool)
        .await
        .unwrap();

    // Insert and query each table
    sqlx::query("INSERT INTO providers (provider_id, display_name) VALUES (?, ?)")
        .bind("test")
        .bind("Test")
        .execute(&pool)
        .await
        .unwrap();
    let p: (String,) = sqlx::query_as("SELECT provider_id FROM providers LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(p.0, "test");

    sqlx::query(
        "INSERT INTO models (provider_id, upstream_model_id, public_model_id) VALUES (?, ?, ?)",
    )
    .bind("test")
    .bind("m1")
    .bind("Model 1")
    .execute(&pool)
    .await
    .unwrap();
    let m: (String,) = sqlx::query_as("SELECT upstream_model_id FROM models LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(m.0, "m1");

    sqlx::query("INSERT INTO aliases (alias, target_json) VALUES (?, ?)")
        .bind("my-alias")
        .bind("\"target\"")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO config_audit_log (actor, action) VALUES (?, ?)")
        .bind("test")
        .bind("migration_test")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO request_log (request_id, endpoint_kind, status) VALUES (?, ?, ?)")
        .bind("req-1")
        .bind("chat")
        .bind("success")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO usage_events (request_id, provider_id, model_id, input_tokens, output_tokens, estimated_cost_usd, free_tier) VALUES (?, ?, ?, ?, ?, ?, ?)")
        .bind("req-1").bind("test").bind("m1").bind(10i64).bind(5i64).bind(0.0f64).bind(true).execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO provider_health_events (provider_id, health_state, event_type) VALUES (?, ?, ?)")
        .bind("test").bind("healthy").bind("test").execute(&pool).await.unwrap();

    sqlx::query("INSERT INTO discovery_runs (provider_id, status) VALUES (?, ?)")
        .bind("test")
        .bind("success")
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("INSERT INTO config_snapshots (version, config_json) VALUES (?, ?)")
        .bind("0.1.0")
        .bind("{}")
        .execute(&pool)
        .await
        .unwrap();
}
