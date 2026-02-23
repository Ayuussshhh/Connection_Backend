//! Route definitions and router setup
//!
//! Configures all API routes and middleware.

pub mod auth;
pub mod connection;
pub mod project;
mod database;
mod foreign_key;
pub mod pipeline;
pub mod snapshot;
mod table;

use crate::auth::middleware::auth_middleware;
use crate::config::Settings;
use crate::state::SharedState;
use axum::{
    http::{header, Method},
    routing::{delete, get, post, put},
    Router,
};
use std::time::Duration;
use tower::ServiceBuilder;
use tower_http::{
    compression::CompressionLayer,
    cors::{Any, CorsLayer},
    request_id::MakeRequestUuid,
    trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer},
    ServiceBuilderExt,
};
use tracing::Level;

/// Create the application router with all routes and middleware
pub fn create_router(state: SharedState, settings: &Settings) -> Router {
    // Build CORS layer
    let cors = build_cors_layer(settings);

    // Build tracing/logging layer
    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    // Build middleware stack
    let middleware = ServiceBuilder::new()
        .set_x_request_id(MakeRequestUuid)
        .layer(trace_layer)
        .layer(CompressionLayer::new())
        .layer(cors)
        .propagate_x_request_id();

    // Protected routes that require authentication
    let protected_routes = Router::new()
        // ============================================
        // AUTHENTICATION API (Protected)
        // ============================================
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/role/{user_id}", put(auth::update_role))
        .route("/api/users", get(auth::list_users))
        
        // ============================================
        // PROJECT MANAGEMENT API
        // Workspace/project organization
        // ============================================
        .route("/api/projects", post(project::create_project))
        .route("/api/projects", get(project::list_projects))
        .route("/api/projects/{id}", get(project::get_project))
        .route("/api/projects/{id}", put(project::update_project))
        .route("/api/projects/{id}", delete(project::delete_project))
        .route("/api/projects/{project_id}/connections", post(project::save_connection))
        .route("/api/projects/{project_id}/connections", get(project::list_connections))
        .route("/api/projects/{project_id}/connections/{connection_id}", delete(project::remove_connection))
        .route("/api/projects/{project_id}/connections/{connection_id}/activate", post(project::activate_connection))
        
        // ============================================
        // CONNECTION MANAGEMENT API
        // Connect to any database with connection string
        // ============================================
        .route("/api/connections", post(connection::connect))
        .route("/api/connections", get(connection::list_connections))
        .route("/api/connections/test", post(connection::test_connection))
        .route("/api/connections/active", get(connection::get_active))
        .route("/api/connections/active", post(connection::set_active))
        .route("/api/connections/disconnect-all", post(connection::disconnect_all))
        .route("/api/connections/{id}", get(connection::get_connection))
        .route("/api/connections/{id}", delete(connection::disconnect))
        .route("/api/connections/{id}/introspect", post(connection::introspect))
        
        // Schema API (for active connection)
        .route("/api/schema", get(connection::get_active_schema))
        
        // ============================================
        // GOVERNANCE PIPELINE API
        // Stage 1: Mirror (Introspection & Semantic Map)
        // ============================================
        .route("/api/connections/{id}/semantic-map", post(pipeline::build_semantic_map))
        .route("/api/connections/{id}/drift", get(pipeline::check_drift))
        
        // ============================================
        // Stage 2: Proposals (Schema PRs)
        // ============================================
        .route("/api/proposals", post(pipeline::create_proposal))
        .route("/api/proposals", get(pipeline::list_proposals))
        .route("/api/proposals/{id}", get(pipeline::get_proposal))
        .route("/api/proposals/{id}/changes", post(pipeline::add_change_to_proposal))
        .route("/api/proposals/{id}/migration", post(pipeline::generate_migration))
        .route("/api/proposals/{id}/submit", post(pipeline::submit_for_review))
        .route("/api/proposals/{id}/approve", post(pipeline::approve_proposal))
        .route("/api/proposals/{id}/reject", post(pipeline::reject_proposal))
        .route("/api/proposals/{id}/comments", post(pipeline::add_comment))
        
        // ============================================
        // Stage 3: Risk Analysis
        // ============================================
        .route("/api/proposals/{id}/analyze", post(pipeline::analyze_risk))
        
        // ============================================
        // Stage 4: Execution & Rollback
        // ============================================
        .route("/api/proposals/{id}/execute", post(pipeline::execute_proposal))
        .route("/api/proposals/{id}/rollback", post(pipeline::rollback_proposal))
        
        // ============================================
        // SCHEMA SNAPSHOTS & IMPACT ANALYSIS
        // Core feature: "What breaks if I change this?"
        // ============================================
        .route("/api/connections/{id}/snapshots", post(snapshot::create_snapshot))
        .route("/api/connections/{id}/snapshots", get(snapshot::list_snapshots))
        .route("/api/connections/{id}/snapshots/latest", get(snapshot::get_latest_snapshot))
        .route("/api/connections/{id}/snapshots/{version}", get(snapshot::get_snapshot_version))
        .route("/api/connections/{id}/snapshots/diff", get(snapshot::diff_snapshots))
        .route("/api/connections/{id}/snapshots/{snapshot_id}/baseline", post(snapshot::set_baseline))
        .route("/api/connections/{id}/blast-radius", post(snapshot::analyze_blast_radius))
        .route("/api/connections/{id}/schema-drift", get(snapshot::check_drift))
        .route("/api/rules", get(snapshot::list_rules))
        
        // ============================================
        // Audit Log
        // ============================================
        .route("/api/audit-log", get(pipeline::get_audit_log))
        
        // Apply auth middleware to all protected routes
        .layer(axum::middleware::from_fn(auth_middleware));
    
    // Build the main router
    Router::new()
        // Health check
        .route("/health", get(health_check))
        
        // ============================================
        // AUTHENTICATION API (Public)
        // ============================================
        .route("/api/auth/login", post(auth::login))
        .route("/api/auth/register", post(auth::register))
        .route("/api/auth/refresh", post(auth::refresh))
        
        // Merge protected routes
        .merge(protected_routes)
        
        // ============================================
        // LEGACY: Original database routes (kept for compatibility)
        // These use the old .env-based connection
        // ============================================
        .route("/db/create", post(database::create_database))
        .route("/db/list", get(database::list_databases))
        .route("/db/connect", post(database::connect_database))
        .route("/db/delete", post(database::delete_database))
        .route("/db/disconnect", post(database::disconnect_database))
        .route("/db/status", get(database::connection_status))
        
        // Table routes (work with current active connection)
        .route("/table/create", post(table::create_table))
        .route("/table/list", get(table::list_tables))
        .route("/table/columns", get(table::get_columns))
        
        // Foreign key routes
        .route("/foreignKey/create", post(foreign_key::create_foreign_key))
        .route("/foreignKey/list", get(foreign_key::list_foreign_keys))
        .route("/foreignKey/listAll", get(foreign_key::list_all_foreign_keys))
        .route("/foreignKey/delete", post(foreign_key::delete_foreign_key))
        .route("/foreignKey/primaryKeys", get(foreign_key::get_primary_keys))
        .route("/foreignKey/validateReference", post(foreign_key::validate_reference))
        
        // Apply middleware and state
        .layer(middleware)
        .with_state(state)
}

/// Build CORS layer from settings
fn build_cors_layer(settings: &Settings) -> CorsLayer {
    let origins: Vec<_> = settings
        .cors
        .allowed_origins
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    if origins.is_empty() {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
            .max_age(Duration::from_secs(3600))
    } else {
        CorsLayer::new()
            .allow_origin(origins)
            .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
            .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION, header::ACCEPT])
            .max_age(Duration::from_secs(3600))
    }
}

/// Health check endpoint
async fn health_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "success": true,
        "message": "Server is running fine.",
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "version": env!("CARGO_PKG_VERSION")
    }))
}
