//! Foreign key management route handlers

use crate::db::queries::{
    SqlBuilder, CHECK_CONSTRAINT_EXISTS, GET_ALL_FOREIGN_KEYS, GET_FOREIGN_KEYS, GET_PRIMARY_KEYS,
    VALIDATE_REFERENCE,
};
use crate::error::{validation_error, ApiResult, AppError};
use crate::models::{
    CreateForeignKeyRequest, DeleteForeignKeyRequest, ForeignKeyAllResponse,
    ForeignKeyCreatedResponse, ForeignKeyFullInfo, ForeignKeyInfo, ForeignKeyListResponse,
    ListForeignKeysQuery, MessageResponse, PrimaryKeyResponse, SuccessResponse,
    ValidateReferenceRequest, ValidateReferenceResponse,
};
use crate::state::SharedState;
use axum::{
    extract::{Query, State},
    Json,
};
use tracing::{debug, info, warn};
use validator::Validate;

/// Create a foreign key constraint
pub async fn create_foreign_key(
    State(state): State<SharedState>,
    Json(payload): Json<CreateForeignKeyRequest>,
) -> ApiResult<Json<SuccessResponse<ForeignKeyCreatedResponse>>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let constraint_name = payload.constraint_name();
    debug!(
        "Creating foreign key: {} ({}.{} -> {}.{})",
        constraint_name,
        payload.source_table,
        payload.source_column,
        payload.referenced_table,
        payload.referenced_column
    );

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    // Check if constraint already exists
    let existing = client
        .query(CHECK_CONSTRAINT_EXISTS, &[&constraint_name])
        .await?;

    if !existing.is_empty() {
        return Err(AppError::Conflict(format!(
            "Constraint '{}' already exists",
            constraint_name
        )));
    }

    // Validate that referenced column can be referenced (has unique/pk constraint)
    let valid_ref = client
        .query(
            VALIDATE_REFERENCE,
            &[&payload.referenced_table, &payload.referenced_column],
        )
        .await?;

    if let Some(row) = valid_ref.first() {
        let is_valid: bool = row.get("is_valid");
        if !is_valid {
            warn!(
                "Referenced column {}.{} is not a primary key or unique column",
                payload.referenced_table, payload.referenced_column
            );
            return Err(validation_error(format!(
                "Column '{}.{}' cannot be referenced. It must be a primary key or have a unique constraint.",
                payload.referenced_table, payload.referenced_column
            )));
        }
    }

    // Build and execute ALTER TABLE ADD CONSTRAINT query
    let query = SqlBuilder::add_foreign_key(
        &payload.source_table,
        &constraint_name,
        &payload.source_column,
        &payload.referenced_table,
        &payload.referenced_column,
        payload.on_delete.as_sql(),
        payload.on_update.as_sql(),
    );

    client.execute(&query, &[]).await.map_err(|e| {
        let err_msg = e.to_string();
        if err_msg.contains("does not exist") {
            if err_msg.contains("column") {
                validation_error(format!("Column does not exist: {}", err_msg))
            } else {
                validation_error(format!("Table does not exist: {}", err_msg))
            }
        } else if err_msg.contains("type mismatch") {
            validation_error("Column type mismatch between source and referenced columns")
        } else {
            AppError::Database(e)
        }
    })?;

    info!(
        "Foreign key constraint '{}' created successfully",
        constraint_name
    );

    Ok(Json(SuccessResponse::with_data(
        format!("Foreign key constraint '{}' created successfully.", constraint_name),
        ForeignKeyCreatedResponse {
            constraint: ForeignKeyFullInfo {
                source_table: payload.source_table.clone(),
                name: constraint_name,
                column: payload.source_column.clone(),
                referenced_table: payload.referenced_table.clone(),
                referenced_column: payload.referenced_column.clone(),
                on_update: payload.on_update.to_string(),
                on_delete: payload.on_delete.to_string(),
            },
        },
    )))
}

/// List foreign keys for a specific table
pub async fn list_foreign_keys(
    State(state): State<SharedState>,
    Query(params): Query<ListForeignKeysQuery>,
) -> ApiResult<Json<SuccessResponse<ForeignKeyListResponse>>> {
    let table_name = &params.table_name;

    if table_name.is_empty() {
        return Err(validation_error("Table name is required"));
    }

    debug!("Listing foreign keys for table: {}", table_name);

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    let rows = client.query(GET_FOREIGN_KEYS, &[&table_name]).await?;

    let foreign_keys: Vec<ForeignKeyInfo> = rows
        .iter()
        .map(|row| ForeignKeyInfo {
            name: row.get("constraint_name"),
            column: row.get("column_name"),
            referenced_table: row.get("referenced_table"),
            referenced_column: row.get("referenced_column"),
            on_update: row.get("update_rule"),
            on_delete: row.get("delete_rule"),
        })
        .collect();

    info!(
        "Listed {} foreign keys for table '{}'",
        foreign_keys.len(),
        table_name
    );

    Ok(Json(SuccessResponse::with_data(
        "Foreign keys fetched successfully.",
        ForeignKeyListResponse { foreign_keys },
    )))
}

/// List all foreign keys in the database
pub async fn list_all_foreign_keys(
    State(state): State<SharedState>,
) -> ApiResult<Json<SuccessResponse<ForeignKeyAllResponse>>> {
    debug!("Listing all foreign keys in database");

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    let rows = client.query(GET_ALL_FOREIGN_KEYS, &[]).await?;

    let foreign_keys: Vec<ForeignKeyFullInfo> = rows
        .iter()
        .map(|row| ForeignKeyFullInfo {
            source_table: row.get("table_name"),
            name: row.get("constraint_name"),
            column: row.get("column_name"),
            referenced_table: row.get("referenced_table"),
            referenced_column: row.get("referenced_column"),
            on_update: row.get("update_rule"),
            on_delete: row.get("delete_rule"),
        })
        .collect();

    info!("Listed {} foreign keys in database", foreign_keys.len());

    Ok(Json(SuccessResponse::with_data(
        "All foreign keys fetched successfully.",
        ForeignKeyAllResponse { foreign_keys },
    )))
}

/// Delete a foreign key constraint
pub async fn delete_foreign_key(
    State(state): State<SharedState>,
    Json(payload): Json<DeleteForeignKeyRequest>,
) -> ApiResult<Json<MessageResponse>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let table_name = &payload.table_name;
    let constraint_name = &payload.constraint_name;

    debug!(
        "Deleting foreign key constraint '{}' from table '{}'",
        constraint_name, table_name
    );

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    // Build and execute DROP CONSTRAINT query
    let query = SqlBuilder::drop_constraint(table_name, constraint_name);

    client.execute(&query, &[]).await.map_err(|e| {
        let err_msg = e.to_string();
        if err_msg.contains("does not exist") {
            AppError::NotFound(format!(
                "Constraint '{}' does not exist on table '{}'",
                constraint_name, table_name
            ))
        } else {
            AppError::Database(e)
        }
    })?;

    info!(
        "Foreign key constraint '{}' deleted from table '{}'",
        constraint_name, table_name
    );

    Ok(Json(MessageResponse::new(format!(
        "Foreign key constraint '{}' deleted successfully.",
        constraint_name
    ))))
}

/// Get primary keys for a table
pub async fn get_primary_keys(
    State(state): State<SharedState>,
    Query(params): Query<ListForeignKeysQuery>,
) -> ApiResult<Json<SuccessResponse<PrimaryKeyResponse>>> {
    let table_name = &params.table_name;

    if table_name.is_empty() {
        return Err(validation_error("Table name is required"));
    }

    debug!("Getting primary keys for table: {}", table_name);

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    let rows = client.query(GET_PRIMARY_KEYS, &[&table_name]).await?;

    let primary_keys: Vec<String> = rows.iter().map(|row| row.get("column_name")).collect();

    info!(
        "Listed {} primary keys for table '{}'",
        primary_keys.len(),
        table_name
    );

    Ok(Json(SuccessResponse::with_data(
        "Primary keys fetched successfully.",
        PrimaryKeyResponse { primary_keys },
    )))
}

/// Validate if a column can be referenced as a foreign key
pub async fn validate_reference(
    State(state): State<SharedState>,
    Json(payload): Json<ValidateReferenceRequest>,
) -> ApiResult<Json<SuccessResponse<ValidateReferenceResponse>>> {
    // Validate input
    payload.validate().map_err(|e| validation_error(e.to_string()))?;

    let table_name = &payload.table_name;
    let column_name = &payload.column_name;

    debug!(
        "Validating reference for column {}.{}",
        table_name, column_name
    );

    // Get current database pool
    let pool = state.db.current_pool().await?;
    let client = pool.get().await?;

    let rows = client
        .query(VALIDATE_REFERENCE, &[&table_name, &column_name])
        .await?;

    let is_valid = rows.first().map(|r| r.get("is_valid")).unwrap_or(false);

    let message = if is_valid {
        "Column can be referenced as a foreign key"
    } else {
        "Column cannot be referenced (must be primary key or unique)"
    };

    info!(
        "Reference validation for {}.{}: {}",
        table_name, column_name, is_valid
    );

    Ok(Json(SuccessResponse::with_data(
        message,
        ValidateReferenceResponse { is_valid },
    )))
}
