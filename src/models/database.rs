//! Database-related models and DTOs

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Request to create a new database
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "PascalCase")]
pub struct CreateDatabaseRequest {
    #[validate(length(min = 1, max = 63, message = "Database name must be between 1 and 63 characters"))]
    #[validate(custom(function = "validate_identifier"))]
    pub name: String,
}

/// Request to connect to a database
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ConnectDatabaseRequest {
    #[validate(length(min = 1, max = 63, message = "Database name is required"))]
    pub db_name: String,
    pub user: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
}

/// Request to delete a database
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct DeleteDatabaseRequest {
    #[validate(length(min = 1, message = "Database name is required"))]
    pub database_name: String,
}

/// Response containing list of databases
#[derive(Debug, Serialize)]
pub struct DatabaseListResponse {
    pub databases: Vec<String>,
}

/// Information about a connected database
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub database: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Validate PostgreSQL identifier
fn validate_identifier(name: &str) -> Result<(), validator::ValidationError> {
    // PostgreSQL identifiers must start with a letter or underscore
    // and contain only letters, digits, underscores, and dollar signs
    let re = regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_$]*$").unwrap();
    
    if !re.is_match(name) {
        let mut err = validator::ValidationError::new("invalid_identifier");
        err.message = Some("Invalid database name. Must start with a letter or underscore and contain only letters, digits, underscores.".into());
        return Err(err);
    }
    
    // Check for reserved words
    let reserved = ["template0", "template1"];
    if reserved.contains(&name.to_lowercase().as_str()) {
        let mut err = validator::ValidationError::new("reserved_name");
        err.message = Some("Cannot use reserved database name".into());
        return Err(err);
    }
    
    Ok(())
}
