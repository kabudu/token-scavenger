use crate::app::state::AppState;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{SqlitePool, migrate::MigrateDatabase};
use std::path::Path;
use std::str::FromStr;
use tracing::info;

/// Open the SQLite database, run migrations, and return a connection pool.
pub async fn init_db(db_path: &str) -> Result<SqlitePool, Box<dyn std::error::Error>> {
    if let Some(parent) = Path::new(db_path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let db_url = format!("sqlite://{}", db_path);
    if !Path::new(db_path).exists() {
        sqlx::sqlite::Sqlite::create_database(&db_url).await?;
        info!("Created database at {}", db_path);
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(
            SqliteConnectOptions::from_str(&db_url)?
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(5))
                .create_if_missing(true),
        )
        .await?;

    sqlx::migrate!("src/db/migrations").run(&pool).await?;

    info!("Database migrations applied successfully");
    Ok(pool)
}

/// Get audit log entries for the admin API.
pub async fn get_audit_entries(state: &AppState) -> serde_json::Value {
    let result = sqlx::query_as::<_, (i64, String, String, String, Option<String>)>(
        "SELECT id, created_at, actor, action, target_type FROM config_audit_log ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await;

    match result {
        Ok(rows) => {
            let entries: Vec<serde_json::Value> = rows
                .into_iter()
                .map(|(id, created, actor, action, target)| {
                    serde_json::json!({
                        "id": id,
                        "created_at": created,
                        "actor": actor,
                        "action": action,
                        "target_type": target,
                    })
                })
                .collect();
            serde_json::json!({"entries": entries})
        }
        Err(_) => serde_json::json!({"entries": []}),
    }
}
