use tokenscavenger::app::state::AppState;
use tokenscavenger::config::schema::Config;
use std::path::PathBuf;

#[tokio::main]
async fn main() {
    let db_path = "/Users/kabudu/.config/tokenscavenger/tokenscavenger.db";
    let db = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path)).await.unwrap();
    let config = Config::default();
    let state = AppState::new(config, db, PathBuf::from("config.toml"), tokio::sync::broadcast::channel(1).0);

    let traffic = tokenscavenger::usage::aggregation::get_hourly_traffic(&state).await;
    println!("Traffic: {}", traffic);

    let dist = tokenscavenger::usage::aggregation::get_provider_distribution(&state).await;
    println!("Distribution: {}", dist);
    
    let series = tokenscavenger::usage::aggregation::get_usage_series(&state).await;
    println!("Series: {}", series);

    let avg_latency: i64 = sqlx::query_as("SELECT COALESCE(CAST(AVG(latency_ms) AS INTEGER), 0) FROM request_log")
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .map(|row: (i64,)| row.0)
        .unwrap_or(0);
    println!("Avg Latency: {}", avg_latency);
}
