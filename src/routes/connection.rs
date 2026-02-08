//! Connection management route handlers
//!
//! Handles dynamic database connections via connection strings.

use crate::connection::{ConnectionInfo, ConnectionTestResult, Environment};
use crate::error::{validation_error, ApiResult, AppError};
use crate::introspection::{PostgresIntrospector, SchemaSnapshot};
use crate::models::{MessageResponse, SuccessResponse};
use crate::state::SharedState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use uuid::Uuid;
use validator::Validate;

/// Request to connect using a connection string
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ConnectRequest {
    /// PostgreSQL connection string: postgres://user:password@host:port/database
    #[validate(length(min = 10, message = "Connection string is required"))]
    pub connection_string: String,
    
    /// Optional friendly name for this connection
    pub name: Option<String>,
    
    /// Environment classification
    pub environment: Option<Environment>,
}

/// Response for successful connection
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectResponse {
    pub connection: ConnectionInfo,
    pub schema: SchemaSnapshot,
}

/// Connect to a database using a connection string
pub async fn connect(
    State(state): State<SharedState>,
    Json(payload): Json<ConnectRequest>,
) -> ApiResult<Json<SuccessResponse<ConnectResponse>>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    debug!("Connecting to database with connection string");

    // Connect to the database
    let conn_info = state.connections.connect(
        &payload.connection_string,
        payload.name,
        payload.environment,
    ).await?;

    info!("Successfully connected to '{}' ({})", conn_info.database, conn_info.id);

    // Introspect the schema
    let pool = state.connections.get_pool(conn_info.id).await?;
    let schema = PostgresIntrospector::introspect(&pool, conn_info.id).await?;

    info!("Introspected {} tables, {} foreign keys", 
        schema.tables.len(), 
        schema.foreign_keys.len()
    );

    Ok(Json(SuccessResponse::with_data(
        format!("Successfully connected to '{}'.", conn_info.database),
        ConnectResponse {
            connection: conn_info,
            schema,
        },
    )))
}

/// Request to test a connection without adding it
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionRequest {
    #[validate(length(min = 10, message = "Connection string is required"))]
    pub connection_string: String,
}

/// Test a connection without adding it
pub async fn test_connection(
    Json(payload): Json<TestConnectionRequest>,
) -> ApiResult<Json<SuccessResponse<ConnectionTestResult>>> {
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    debug!("Testing connection");

    let result = crate::connection::ConnectionManager::test_connection(
        &payload.connection_string
    ).await?;

    Ok(Json(SuccessResponse::with_data(
        "Connection test successful.".to_string(),
        result,
    )))
}

/// List all active connections
pub async fn list_connections(
    State(state): State<SharedState>,
) -> ApiResult<Json<SuccessResponse<Vec<ConnectionInfo>>>> {
    let connections = state.connections.list_connections().await;
    
    Ok(Json(SuccessResponse::with_data(
        format!("{} active connection(s).", connections.len()),
        connections,
    )))
}

/// Request to get a specific connection
#[derive(Debug, Deserialize)]
pub struct ConnectionIdPath {
    pub id: Uuid,
}

/// Get a specific connection by ID
pub async fn get_connection(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<SuccessResponse<ConnectionInfo>>> {
    let conn = state.connections.get_connection(id).await
        .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", id)))?;
    
    let info = ConnectionInfo::from(conn.as_ref());
    
    Ok(Json(SuccessResponse::with_data(
        "Connection retrieved.".to_string(),
        info,
    )))
}

/// Disconnect from a specific database
pub async fn disconnect(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<MessageResponse>> {
    state.connections.disconnect(id).await?;
    
    info!("Disconnected from connection {}", id);

    Ok(Json(MessageResponse::new(format!(
        "Disconnected from connection {} successfully.",
        id
    ))))
}

/// Disconnect from all databases
pub async fn disconnect_all(
    State(state): State<SharedState>,
) -> ApiResult<Json<MessageResponse>> {
    let count = state.connections.connection_count().await;
    state.connections.disconnect_all().await;
    
    info!("Disconnected from all {} connection(s)", count);

    Ok(Json(MessageResponse::new(format!(
        "Disconnected from {} connection(s).",
        count
    ))))
}

/// Set the active connection
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetActiveRequest {
    pub connection_id: Uuid,
}

pub async fn set_active(
    State(state): State<SharedState>,
    Json(payload): Json<SetActiveRequest>,
) -> ApiResult<Json<MessageResponse>> {
    state.connections.set_active_connection(payload.connection_id).await?;
    
    Ok(Json(MessageResponse::new(format!(
        "Active connection set to {}.",
        payload.connection_id
    ))))
}

/// Get the currently active connection
pub async fn get_active(
    State(state): State<SharedState>,
) -> ApiResult<Json<serde_json::Value>> {
    match state.connections.get_active_connection().await {
        Some(conn) => {
            let info = ConnectionInfo::from(conn.as_ref());
            Ok(Json(serde_json::json!({
                "success": true,
                "hasActiveConnection": true,
                "connection": info
            })))
        }
        None => {
            Ok(Json(serde_json::json!({
                "success": true,
                "hasActiveConnection": false,
                "message": "No active connection."
            })))
        }
    }
}

/// Introspect/refresh schema for a connection
pub async fn introspect(
    State(state): State<SharedState>,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult<Json<SuccessResponse<SchemaSnapshot>>> {
    let pool = state.connections.get_pool(id).await?;
    let schema = PostgresIntrospector::introspect(&pool, id).await?;
    
    info!("Re-introspected connection {}: {} tables", id, schema.tables.len());

    Ok(Json(SuccessResponse::with_data(
        format!("Schema introspected: {} tables.", schema.tables.len()),
        schema,
    )))
}

/// Get current schema for the active connection
pub async fn get_active_schema(
    State(state): State<SharedState>,
) -> ApiResult<Json<SuccessResponse<SchemaSnapshot>>> {
    let conn = state.connections.get_active_connection().await
        .ok_or_else(|| AppError::NotConnected("No active connection".to_string()))?;
    
    let id = conn.id;
    let pool = state.connections.get_pool(id).await?;
    let schema = PostgresIntrospector::introspect(&pool, id).await?;
    
    Ok(Json(SuccessResponse::with_data(
        format!("Schema for '{}': {} tables.", conn.params.database, schema.tables.len()),
        schema,
    )))
}
