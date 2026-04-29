use crate::app::state::AppState;
use std::time::Duration;
use tokio::signal;
use tracing::info;

/// Graceful shutdown handler. Waits for SIGINT/SIGTERM, then drains in-flight requests,
/// cancels background tasks, flushes buffers, and closes the database.
pub async fn shutdown(state: AppState) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
        info!("Received SIGINT (Ctrl+C)");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM handler")
            .recv()
            .await;
        info!("Received SIGTERM");
    };

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    info!("Starting graceful shutdown...");

    // Signal background tasks to stop
    let _ = state.shutdown_tx.send(true);

    // Give in-flight requests time to complete
    tokio::time::sleep(Duration::from_secs(5)).await;

    info!("Closing database pool...");
    state.db.close().await;
    info!("Shutdown complete");
}
