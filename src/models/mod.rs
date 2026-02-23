//! Data models and DTOs (Data Transfer Objects)
//!
//! Contains all request/response structures used by the API.

pub mod database;
pub mod foreign_key;
pub mod project;
pub mod table;

// Re-export commonly used types
pub use database::*;
pub use foreign_key::*;
pub use project::*;
pub use table::*;

use serde::Serialize;

/// Generic success response
#[derive(Serialize)]
pub struct SuccessResponse<T: Serialize> {
    pub success: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T: Serialize> SuccessResponse<T> {
    #[allow(dead_code)]
    pub fn new(message: impl Into<String>, data: Option<T>) -> Self {
        Self {
            success: true,
            message: message.into(),
            data,
        }
    }

    pub fn with_data(message: impl Into<String>, data: T) -> Self {
        Self {
            success: true,
            message: message.into(),
            data: Some(data),
        }
    }

    #[allow(dead_code)]
    pub fn message_only(message: impl Into<String>) -> SuccessResponse<()> {
        SuccessResponse {
            success: true,
            message: message.into(),
            data: None,
        }
    }
}

/// Message-only response (no data)
#[derive(Serialize)]
pub struct MessageResponse {
    pub message: String,
}

impl MessageResponse {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

