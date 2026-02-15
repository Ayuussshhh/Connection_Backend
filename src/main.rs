//! Interactive Database API - A high-performance PostgreSQL management service
//!
//! This is the main entry point for the application.
//! 
//! NEW ARCHITECTURE: The server now supports dynamic database connections.
//! You no longer need to configure a database in .env - users can connect
//! to any database by providing a connection string via the API.
//!
//! GOVERNANCE PIPELINE: The server now includes a full governance pipeline:
//! - Stage 1 (Mirror): Schema introspection with semantic mapping
//! - Stage 2 (Proposal): Schema change proposals (like GitHub PRs)
//! - Stage 3 (Brain): Risk analysis and safety scoring
//! - Stage 4 (Orchestrator): Safe execution with rollback capability

mod config;
mod connection;
mod db;
mod error;
mod introspection;
mod models;
mod pipeline;
mod routes;
mod state;

use crate::config::Settings;
use crate::db::DatabaseManager;
use crate::routes::create_router;
use crate::state::AppState;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize tracing subscriber for structured logging
    init_tracing();

    info!("🚀 Starting SchemaFlow - Interactive Database Platform...");

    // Load configuration
    let settings = Settings::load()?;
    info!("📋 Configuration loaded successfully");

    // Try to initialize legacy database manager (optional)
    // If .env has database config, use it for backward compatibility
    let state = match DatabaseManager::new(&settings.database).await {
        Ok(db_manager) => {
            info!("🔌 Legacy database connection established (from .env)");
            Arc::new(AppState::with_legacy_db(db_manager))
        }
        Err(e) => {
            warn!("⚠️  No legacy database configured: {}", e);
            info!("💡 Server starting without pre-configured database.");
            info!("   Use POST /api/connections to connect to any database.");
            Arc::new(AppState::new())
        }
    };

    // Build the router
    let app = create_router(state, &settings);

    // Create socket address
    let addr = SocketAddr::from((settings.server.host, settings.server.port));
    
    info!("🌐 Server listening on http://{}", addr);
    info!("");
    info!("📚 API Endpoints:");
    info!("   ─── Connection Management ───");
    info!("   POST /api/connections          - Connect to a database");
    info!("   GET  /api/connections          - List all connections");
    info!("   POST /api/connections/test     - Test a connection");
    info!("   GET  /api/schema               - Get schema for active connection");
    info!("");
    info!("   ─── Governance Pipeline ───");
    info!("   POST /api/pipeline/mirror      - Build semantic map (Stage 1)");
    info!("   POST /api/pipeline/drift       - Check for schema drift");
    info!("   POST /api/pipeline/proposals   - Create new proposal (Stage 2)");
    info!("   GET  /api/pipeline/proposals   - List all proposals");
    info!("   POST /api/pipeline/proposals/:id/changes   - Add change to proposal");
    info!("   POST /api/pipeline/proposals/:id/migration - Generate migration SQL");
    info!("   POST /api/pipeline/proposals/:id/submit    - Submit for review");
    info!("   POST /api/pipeline/proposals/:id/approve   - Approve proposal");
    info!("   POST /api/pipeline/proposals/:id/reject    - Reject proposal");
    info!("   POST /api/pipeline/risk        - Analyze risk (Stage 3)");
    info!("   POST /api/pipeline/execute     - Execute proposal (Stage 4)");
    info!("   POST /api/pipeline/rollback    - Rollback execution");
    info!("   GET  /api/pipeline/audit       - View audit log");
    info!("");

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
