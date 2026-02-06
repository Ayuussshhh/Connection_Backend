//! Route definitions and router setup
//!
//! Configures all API routes and middleware.

mod database;
mod foreign_key;
mod table;

use crate::config::Settings;
use crate::state::SharedState;
use axum::{
    http::{header, Method},
    routing::{get, post},
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

    // Build the router
    Router::new()
        // Health check
        .route("/health", get(health_check))
        
        // Database routes
        .route("/db/create", post(database::create_database))
        .route("/db/list", get(database::list_databases))
        .route("/db/connect", post(database::connect_database))
        .route("/db/delete", post(database::delete_database))
        .route("/db/disconnect", post(database::disconnect_database))
        .route("/db/status", get(database::connection_status))
        
        // Table routes
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
