//! SchemaFlow API - Database Governance Platform
//!
//! A "GitHub PR for Databases" - propose, review, and safely execute schema changes.
//! 
//! NEW ARCHITECTURE: The server now supports dynamic database connections.
//! You no longer need to configure a database in .env - users can connect
//! to any database by providing a connection string via the API.
//!
//! GOVERNANCE PIPELINE: The server includes a full governance workflow:
//! - Stage 1 (Mirror): Schema introspection with semantic mapping
//! - Stage 2 (Proposal): Schema change proposals with review workflow
//! - Stage 3 (Simulate): Risk analysis, dry-run validation, impact assessment
//! - Stage 4 (Execute): Safe execution with rollback capability

mod auth;
mod config;
mod connection;
mod db;
mod error;
mod introspection;
mod models;
mod pipeline;
mod proposal;
mod routes;
mod simulation;
mod snapshot;
mod state;
mod users;

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

    info!("🚀 Starting SchemaFlow - Database Governance Platform...");

    // Load configuration
    let settings = Settings::load()?;
    info!("📋 Configuration loaded successfully");
    
    // Get JWT secret from environment or generate a default (for dev only)
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| {
            warn!("⚠️  JWT_SECRET not set, using default (INSECURE - set in production!)");
            "schemaflow-dev-secret-change-in-production".to_string()
        });

    // Try to initialize legacy database manager (optional)
    // If .env has database config, use it for backward compatibility
    let state = match DatabaseManager::new(&settings.database).await {
        Ok(db_manager) => {
            info!("🔌 Legacy database connection established (from .env)");
            Arc::new(AppState::with_legacy_db(db_manager, jwt_secret))
        }
        Err(e) => {
            warn!("⚠️  No legacy database configured: {}", e);
            info!("💡 Server starting without pre-configured database.");
            info!("   Use POST /api/connections to connect to any database.");
            Arc::new(AppState::new(jwt_secret))
        }
    };
    
    // Initialize default admin user
    if let Err(e) = state.users.init_default_admin().await {
        warn!("⚠️  Failed to initialize default admin: {}", e);
    } else {
        info!("👤 Default admin user initialized (admin@schemaflow.local / admin123)");
    }

    // Build the router
    let app = create_router(state, &settings);

    // Create socket address
    let addr = SocketAddr::from((settings.server.host, settings.server.port));
    
    info!("🌐 Server listening on http://{}", addr);
    info!("");
    info!("📚 API Endpoints:");
    info!("   ─── Authentication ───");
    info!("   POST /api/auth/login           - Login with email/password");
    info!("   POST /api/auth/register        - Register new account");
    info!("   POST /api/auth/refresh         - Refresh access token");
    info!("   GET  /api/auth/me              - Get current user");
    info!("");
    info!("   ─── Connection Management ───");
    info!("   POST /api/connections          - Connect to a database");
    info!("   GET  /api/connections          - List all connections");
    info!("   POST /api/connections/test     - Test a connection");
    info!("   GET  /api/schema               - Get schema for active connection");
    info!("");
    info!("   ─── Governance Pipeline ───");
    info!("   POST /api/proposals            - Create new proposal");
    info!("   GET  /api/proposals            - List all proposals");
    info!("   POST /api/proposals/:id/submit - Submit for review");
    info!("   POST /api/proposals/:id/approve - Approve (Admin only)");
    info!("   POST /api/proposals/:id/analyze - Risk analysis");
    info!("   POST /api/proposals/:id/execute - Execute migration");
    info!("");
    info!("   ─── Impact Analysis (Core Feature) ───");
    info!("   POST /api/connections/:id/snapshots    - Create schema snapshot");
    info!("   GET  /api/connections/:id/snapshots    - List all snapshots");
    info!("   GET  /api/connections/:id/snapshots/diff - Compare snapshots");
    info!("   POST /api/connections/:id/blast-radius - Analyze impact of changes");
    info!("   GET  /api/connections/:id/schema-drift - Check drift from baseline");
    info!("   GET  /api/rules                        - List governance rules");
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
