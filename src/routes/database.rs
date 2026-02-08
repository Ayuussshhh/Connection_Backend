//! Database management route handlers
//! 
//! LEGACY: These routes require a database to be configured in .env
//! For dynamic connections, use the /api/connections endpoints instead.

use crate::db::queries::{SqlBuilder, LIST_DATABASES};
use crate::error::{validation_error, ApiResult, AppError};
use crate::models::{
    ConnectDatabaseRequest, ConnectionInfo, CreateDatabaseRequest, DatabaseListResponse,
    DeleteDatabaseRequest, MessageResponse, SuccessResponse,
};
use crate::state::SharedState;
use axum::{extract::State, Json};
use tracing::{debug, info};
use validator::Validate;

/// Helper to get the legacy database manager or return an error
fn get_legacy_db(state: &SharedState) -> Result<&crate::db::DatabaseManager, AppError> {
    state.db.as_ref().ok_or_else(|| {
        AppError::NotConnected(
            "No database configured in .env. Use POST /api/connections to connect dynamically.".to_string()
        )
    })
}

/// Create a new database
pub async fn create_database(
    State(state): State<SharedState>,
    Json(payload): Json<CreateDatabaseRequest>,
) -> ApiResult<Json<MessageResponse>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let db_name = &payload.name;
    debug!("Creating database: {}", db_name);

    // Get admin pool client
    let db = get_legacy_db(&state)?;
    let client = db.admin_pool().get().await?;

    // Execute CREATE DATABASE (cannot be parameterized)
    let query = SqlBuilder::create_database(db_name);
    client.execute(&query, &[]).await.map_err(|e| {
        if e.to_string().contains("already exists") {
            AppError::Conflict(format!("Database '{}' already exists", db_name))
        } else {
            AppError::Database(e)
        }
    })?;

    info!("Database '{}' created successfully", db_name);

    Ok(Json(MessageResponse::new(format!(
        "Database '{}' created successfully.",
        db_name
    ))))
}

/// List all databases
pub async fn list_databases(
    State(state): State<SharedState>,
) -> ApiResult<Json<SuccessResponse<DatabaseListResponse>>> {
    debug!("Listing all databases");

    let db = get_legacy_db(&state)?;
    let client = db.admin_pool().get().await?;
    let rows = client.query(LIST_DATABASES, &[]).await?;

    let databases: Vec<String> = rows.iter().map(|row| row.get("datname")).collect();

    info!("Listed {} databases", databases.len());

    Ok(Json(SuccessResponse::with_data(
        "Databases listed successfully.",
        DatabaseListResponse { databases },
    )))
}

/// Connect to a specific database
pub async fn connect_database(
    State(state): State<SharedState>,
    Json(payload): Json<ConnectDatabaseRequest>,
) -> ApiResult<Json<SuccessResponse<ConnectionInfo>>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let db_name = &payload.db_name;
    debug!("Connecting to database: {}", db_name);

    let db = get_legacy_db(&state)?;
    
    // Connect to the database
    let conn_info = db
        .connect(
            db_name,
            payload.user.as_deref(),
            payload.password.as_deref(),
            payload.host.as_deref(),
            payload.port,
        )
        .await?;

    info!("Successfully connected to database '{}'", db_name);

    Ok(Json(SuccessResponse::with_data(
        format!("Successfully connected to '{}'.", db_name),
        ConnectionInfo {
            database: conn_info.database,
            host: conn_info.host,
            port: conn_info.port,
            user: conn_info.user,
            connected_at: conn_info.connected_at,
        },
    )))
}

/// Delete a database
pub async fn delete_database(
    State(state): State<SharedState>,
    Json(payload): Json<DeleteDatabaseRequest>,
) -> ApiResult<Json<MessageResponse>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let db_name = &payload.database_name;
    debug!("Deleting database: {}", db_name);

    let db = get_legacy_db(&state)?;
    
    // Check if trying to delete currently connected database
    if let Some(conn) = db.connection_info().await {
        if conn.database == *db_name {
            return Err(validation_error(
                "Cannot delete currently connected database. Please disconnect first.",
            ));
        }
    }

    // Get admin pool client
    let client = db.admin_pool().get().await?;

    // Execute DROP DATABASE
    let query = SqlBuilder::drop_database(db_name);
    client.execute(&query, &[]).await.map_err(|e| {
        if e.to_string().contains("does not exist") {
            AppError::NotFound(format!("Database '{}' does not exist", db_name))
        } else if e.to_string().contains("being accessed") {
            AppError::Conflict(format!(
                "Cannot delete database '{}': other sessions are connected",
                db_name
            ))
        } else {
            AppError::Database(e)
        }
    })?;

    info!("Database '{}' deleted successfully", db_name);

    Ok(Json(MessageResponse::new(format!(
        "Database '{}' deleted successfully.",
        db_name
    ))))
}

/// Disconnect from current database
pub async fn disconnect_database(
    State(state): State<SharedState>,
) -> ApiResult<Json<MessageResponse>> {
    let db = get_legacy_db(&state)?;
    
    if !db.is_connected().await {
        return Err(AppError::NotConnected("No database is currently connected.".to_string()));
    }

    let db_name = db
        .connection_info()
        .await
        .map(|c| c.database)
        .unwrap_or_default();

    db.disconnect().await;

    info!("Disconnected from database '{}'", db_name);

    Ok(Json(MessageResponse::new(format!(
        "Disconnected from '{}' successfully.",
        db_name
    ))))
}

/// Get current connection status
pub async fn connection_status(
    State(state): State<SharedState>,
) -> ApiResult<Json<serde_json::Value>> {
    let db = match state.db.as_ref() {
        Some(db) => db,
        None => {
            return Ok(Json(serde_json::json!({
                "success": true,
                "connected": false,
                "message": "No database configured. Use POST /api/connections to connect."
            })));
        }
    };
    
    match db.connection_info().await {
        Some(conn) => Ok(Json(serde_json::json!({
            "success": true,
            "connected": true,
            "connection": {
                "database": conn.database,
                "host": conn.host,
                "port": conn.port,
                "user": conn.user,
                "connectedAt": conn.connected_at.to_rfc3339()
            }
        }))),
        None => Ok(Json(serde_json::json!({
            "success": true,
            "connected": false,
            "message": "No database is currently connected."
        }))),
    }
}
