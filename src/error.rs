//! Error handling module
//!
//! Provides unified error types and handling for the entire application.

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use thiserror::Error;
use tracing::error;

/// Application-wide error type
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    Database(#[from] tokio_postgres::Error),

    #[error("Pool error: {0}")]
    Pool(#[from] deadpool_postgres::PoolError),

    #[error("Connection not established: {0}")]
    NotConnected(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

/// Error response structure
#[derive(Serialize)]
pub struct ErrorResponse {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error_code, message, details) = match &self {
            AppError::Database(e) => {
                error!("Database error: {:?}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "DATABASE_ERROR",
                    "A database error occurred".to_string(),
                    Some(e.to_string()),
                )
            }
            AppError::Pool(e) => {
                error!("Pool error: {:?}", e);
                (
                    StatusCode::SERVICE_UNAVAILABLE,
                    "POOL_EXHAUSTED",
                    "Database connection pool exhausted".to_string(),
                    Some(e.to_string()),
                )
            }
            AppError::NotConnected(msg) => (
                StatusCode::BAD_REQUEST,
                "NOT_CONNECTED",
                msg.clone(),
                None,
            ),
            AppError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                msg.clone(),
                None,
            ),
            AppError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "NOT_FOUND",
                msg.clone(),
                None,
            ),
            AppError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "CONFLICT",
                msg.clone(),
                None,
            ),
            AppError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "BAD_REQUEST",
                msg.clone(),
                None,
            ),
            AppError::Internal(msg) => {
                error!("Internal error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "INTERNAL_ERROR",
                    "An internal error occurred".to_string(),
                    Some(msg.clone()),
                )
            }
            AppError::Config(msg) => {
                error!("Configuration error: {}", msg);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "CONFIG_ERROR",
                    "A configuration error occurred".to_string(),
                    Some(msg.clone()),
                )
            }
        };

        let body = Json(ErrorResponse {
            success: false,
            message,
            error: details,
            code: Some(error_code.to_string()),
        });

        (status, body).into_response()
    }
}

/// Result type alias for API handlers
pub type ApiResult<T> = Result<T, AppError>;

/// Helper function to create a validation error
pub fn validation_error(msg: impl Into<String>) -> AppError {
    AppError::Validation(msg.into())
}

/// Helper function to create a not found error
pub fn not_found_error(msg: impl Into<String>) -> AppError {
    AppError::NotFound(msg.into())
}

/// Helper function to create a conflict error
pub fn conflict_error(msg: impl Into<String>) -> AppError {
    AppError::Conflict(msg.into())
}
