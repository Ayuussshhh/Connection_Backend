//! Table management route handlers
//!
//! These routes work with the currently active database connection.
//! Use POST /api/connections to connect to a database first.

use crate::db::queries::{SqlBuilder, GET_COLUMNS, LIST_TABLES};
use crate::error::{validation_error, ApiResult, AppError};
use crate::models::{
    ColumnInfo, ColumnListResponse, CreateTableRequest, ListColumnsQuery, MessageResponse,
    SuccessResponse, TableInfo, TableListResponse,
};
use crate::state::SharedState;
use axum::{
    extract::{Query, State},
    Json,
};
use deadpool_postgres::Pool;
use tracing::{debug, info};
use validator::Validate;

/// Helper to get the active database pool
async fn get_active_pool(state: &SharedState) -> Result<Pool, AppError> {
    // Try the connection manager
    state.connections.get_active_pool().await
        .map_err(|_| AppError::NotConnected(
            "No active database connection. Use POST /api/connections to connect.".to_string()
        ))
}

/// Create a new table
pub async fn create_table(
    State(state): State<SharedState>,
    Json(payload): Json<CreateTableRequest>,
) -> ApiResult<Json<MessageResponse>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    // Validate column data types
    for col in &payload.columns {
        col.validate_data_type()
            .map_err(|e| validation_error(e))?;
    }

    let table_name = &payload.table_name;
    debug!("Creating table: {} with {} columns", table_name, payload.columns.len());

    // Get current database pool
    let pool = get_active_pool(&state).await?;
    let client = pool.get().await?;

    // Build column definitions
    let column_defs: Vec<String> = payload.columns.iter().map(|col| col.to_sql()).collect();
    let column_defs_str = column_defs.join(", ");

    // Build and execute CREATE TABLE query
    let query = SqlBuilder::create_table(table_name, &column_defs_str);
    
    client.execute(&query, &[]).await.map_err(|e| {
        let err_msg = e.to_string();
        if err_msg.contains("already exists") {
            AppError::Conflict(format!("Table '{}' already exists", table_name))
        } else if err_msg.contains("type") && err_msg.contains("does not exist") {
            validation_error(format!("Invalid column type in table definition: {}", err_msg))
        } else {
            AppError::Database(e)
        }
    })?;

    info!("Table '{}' created successfully with {} columns", table_name, payload.columns.len());

    Ok(Json(MessageResponse::new(format!(
        "Table '{}' created successfully.",
        table_name
    ))))
}

/// List all tables in the current database
pub async fn list_tables(
    State(state): State<SharedState>,
) -> ApiResult<Json<SuccessResponse<TableListResponse>>> {
    debug!("Listing all tables");

    // Get current database pool
    let pool = get_active_pool(&state).await?;
    let client = pool.get().await?;

    let rows = client.query(LIST_TABLES, &[]).await?;

    let tables: Vec<TableInfo> = rows
        .iter()
        .map(|row| TableInfo {
            name: row.get("name"),
            schema: row.get("schema"),
            owner: row.get("owner"),
            table_type: row.get("type"),
        })
        .collect();

    info!("Listed {} tables", tables.len());

    Ok(Json(SuccessResponse::with_data(
        "Tables fetched successfully.",
        TableListResponse { tables },
    )))
}

/// Get columns for a specific table
pub async fn get_columns(
    State(state): State<SharedState>,
    Query(params): Query<ListColumnsQuery>,
) -> ApiResult<Json<SuccessResponse<ColumnListResponse>>> {
    let table_name = &params.table_name;
    
    if table_name.is_empty() {
        return Err(validation_error("Table name is required"));
    }

    debug!("Getting columns for table: {}", table_name);

    // Get current database pool
    let pool = get_active_pool(&state).await?;
    let client = pool.get().await?;

    let rows = client.query(GET_COLUMNS, &[&table_name]).await?;

    if rows.is_empty() {
        return Err(AppError::NotFound(format!(
            "Table '{}' not found or has no columns",
            table_name
        )));
    }

    let columns: Vec<ColumnInfo> = rows
        .iter()
        .map(|row| {
            let nullable: bool = row.get("nullable");
            let max_len: Option<i32> = row.get("character_maximum_length");
            
            ColumnInfo {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                nullable,
                default_value: row.get("column_default"),
                max_length: max_len,
                is_primary_key: row.get("is_primary_key"),
                is_unique: row.get("is_unique"),
            }
        })
        .collect();

    info!("Listed {} columns for table '{}'", columns.len(), table_name);

    Ok(Json(SuccessResponse::with_data(
        "Columns fetched successfully.",
        ColumnListResponse { columns },
    )))
}
