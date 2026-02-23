//! Database management route handlers
//! 
//! DEPRECATED: These legacy endpoints have been replaced with /api/connections
//! for dynamic database connectivity.

use crate::error::{ApiResult, AppError};
use crate::models::{
    CreateDatabaseRequest, DeleteDatabaseRequest, MessageResponse,
};
use crate::state::SharedState;
use axum::extract::State;
use axum::Json;

/// Create a new database - DEPRECATED
pub async fn create_database(
    _state: State<SharedState>,
    _payload: Json<CreateDatabaseRequest>,
) -> ApiResult<Json<MessageResponse>> {
    Err(AppError::NotConnected(
        "Legacy database endpoints are deprecated. Use POST /api/connections for dynamic database connectivity.".to_string()
    ))
}

/// Connect to a specific database - DEPRECATED
pub async fn connect_database(
    _state: State<SharedState>,
    _payload: Json<serde_json::Value>,
) -> ApiResult<Json<MessageResponse>> {
    Err(AppError::NotConnected(
        "Legacy endpoints are deprecated. Use POST /api/connections for dynamic database connectivity.".to_string()
    ))
}

/// List databases - DEPRECATED
pub async fn list_databases(
    _state: State<SharedState>,
) -> ApiResult<Json<MessageResponse>> {
    Err(AppError::NotConnected(
        "Legacy database endpoints are deprecated. Use POST /api/connections for dynamic database connectivity.".to_string()
    ))
}

/// Delete a database - DEPRECATED
pub async fn delete_database(
    _state: State<SharedState>,
    _payload: Json<DeleteDatabaseRequest>,
) -> ApiResult<Json<MessageResponse>> {
    Err(AppError::NotConnected(
        "Legacy database endpoints are deprecated. Use POST /api/connections for dynamic database connectivity.".to_string()
    ))
}

/// Disconnect from database - DEPRECATED
pub async fn disconnect_database(
    _state: State<SharedState>,
) -> ApiResult<Json<MessageResponse>> {
    Err(AppError::NotConnected(
        "Legacy endpoints are deprecated. Use POST /api/connections for dynamic database connectivity.".to_string()
    ))
}

/// Get connection status - DEPRECATED
pub async fn connection_status(
    _state: State<SharedState>,
) -> ApiResult<Json<serde_json::Value>> {
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Use new /api/connections endpoints for dynamic database connectivity."
    })))
}
