use crate::app::state::AppState;
use std::time::Duration;
use tokio::signal;
use tracing::{info, warn};

/// Graceful shutdown handler. Waits for SIGINT/SIGTERM, signals background
/// tasks to stop, waits for them to drain (with timeout), then closes the
/// database and returns so `axum::serve` can finish.
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

    // Signal background tasks to stop.
    let _ = state.shutdown_tx.send(true);

    // Drop the broadcast log sender. This closes the channel, causing all
    // SSE subscribers (e.g. /admin/logs/stream) to see RecvError::Closed
    // and end their streams, breaking the circular dependency where axum
    // waits for SSE connections that wait for the broadcast channel to
    // close that waits for AppState to drop that waits for axum to finish.
    {
        let mut guard = state.log_tx.lock().unwrap();
        guard.take(); // drops the inner broadcast::Sender
    }

    // Drain all background task handles and wait for them to finish.
    // If a task is mid-operation (e.g. discovery network call), it needs
    // time to notice the signal at the top of its next loop iteration.
    let handles: Vec<tokio::task::JoinHandle<()>> = {
        let mut guard = state.background_handles.lock().unwrap();
        guard.drain(..).collect()
    };

    if !handles.is_empty() {
        let bg_timeout = Duration::from_secs(30);
        match tokio::time::timeout(bg_timeout, join_all_handles(handles)).await {
            Ok(_) => info!("All background tasks stopped cleanly"),
            Err(_elapsed) => {
                warn!(
                    "Timed out after {}s waiting for background tasks — some may have been cancelled",
                    bg_timeout.as_secs()
                );
            }
        }
    }

    // Give in-flight HTTP requests a brief window to complete.
    tokio::time::sleep(Duration::from_secs(2)).await;

    info!("Closing database pool...");
    state.db.close().await;
    info!("Shutdown complete");
}

/// Await all join handles sequentially (tasks are already running concurrently).
async fn join_all_handles(handles: Vec<tokio::task::JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.await;
    }
}
