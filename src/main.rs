//! Interactive Database API - A high-performance PostgreSQL management service
//!
//! This is the main entry point for the application.

mod config;
mod db;
mod error;
mod handlers;
mod models;
mod routes;
mod state;

use crate::config::Settings;
use crate::db::DatabaseManager;
use crate::routes::create_router;
use crate::state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing subscriber for structured logging
    init_tracing();

    info!("🚀 Starting Interactive Database API...");

    // Load configuration
    let settings = Settings::load()?;
    info!("📋 Configuration loaded successfully");

    // Initialize database manager
    let db_manager = DatabaseManager::new(&settings.database).await?;
    info!("🔌 Connected to PostgreSQL successfully");

    // Create application state
    let state = Arc::new(AppState::new(db_manager));

    // Build the router
    let app = create_router(state, &settings);

    // Create socket address
    let addr = SocketAddr::from((settings.server.host, settings.server.port));
    info!("🌐 Server listening on http://{}", addr);

    // Create TCP listener and serve
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("👋 Server shutdown complete");
    Ok(())
}

/// Initialize tracing with structured logging
fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,interactive_db_api=debug,tower_http=debug"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_level(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .compact(),
        )
        .init();
}

/// Graceful shutdown signal handler
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("📴 Received Ctrl+C signal, initiating graceful shutdown...");
        },
        _ = terminate => {
            info!("📴 Received terminate signal, initiating graceful shutdown...");
        },
    }
}
