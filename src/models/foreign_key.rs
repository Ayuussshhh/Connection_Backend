//! Foreign key-related models and DTOs

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Referential action for ON DELETE / ON UPDATE
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReferentialAction {
    #[default]
    Restrict,
    Cascade,
    SetNull,
    NoAction,
    SetDefault,
}

impl ReferentialAction {
    pub fn as_sql(&self) -> &'static str {
        match self {
            ReferentialAction::Restrict => "RESTRICT",
            ReferentialAction::Cascade => "CASCADE",
            ReferentialAction::SetNull => "SET NULL",
            ReferentialAction::NoAction => "NO ACTION",
            ReferentialAction::SetDefault => "SET DEFAULT",
        }
    }
}

impl std::fmt::Display for ReferentialAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_sql())
    }
}

/// Request to create a foreign key constraint
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateForeignKeyRequest {
    #[validate(length(min = 1, message = "Source table is required"))]
    pub source_table: String,
    
    #[validate(length(min = 1, message = "Source column is required"))]
    pub source_column: String,
    
    #[validate(length(min = 1, message = "Referenced table is required"))]
    pub referenced_table: String,
    
    #[validate(length(min = 1, message = "Referenced column is required"))]
    pub referenced_column: String,
    
    /// Optional custom constraint name
    pub constraint_name: Option<String>,
    
    #[serde(default)]
    pub on_delete: ReferentialAction,
    
    #[serde(default)]
    pub on_update: ReferentialAction,
}

/// Request to delete a foreign key constraint
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct DeleteForeignKeyRequest {
    #[validate(length(min = 1, message = "Table name is required"))]
    pub table_name: String,
    
    #[validate(length(min = 1, message = "Constraint name is required"))]
    pub constraint_name: String,
}

/// Query parameters for listing foreign keys of a table
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListForeignKeysQuery {
    pub table_name: String,
}

/// Query parameters for validating a reference
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct ValidateReferenceRequest {
    #[validate(length(min = 1, message = "Table name is required"))]
    pub table_name: String,
    
    #[validate(length(min = 1, message = "Column name is required"))]
    pub column_name: String,
}

/// Foreign key constraint information
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: String,
    pub column: String,
    pub referenced_table: String,
    pub referenced_column: String,
    pub on_update: String,
    pub on_delete: String,
}

/// Foreign key information with source table (for listAll)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyFullInfo {
    pub source_table: String,
    pub name: String,
    pub column: String,
    pub referenced_table: String,
    pub referenced_column: String,
    pub on_update: String,
    pub on_delete: String,
}

/// Response containing created constraint details
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyCreatedResponse {
    pub constraint: ForeignKeyFullInfo,
}

/// Response containing list of foreign keys
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyListResponse {
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

/// Response containing all foreign keys
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyAllResponse {
    pub foreign_keys: Vec<ForeignKeyFullInfo>,
}

/// Response containing primary keys
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryKeyResponse {
    pub primary_keys: Vec<String>,
}

/// Response for reference validation
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateReferenceResponse {
    pub is_valid: bool,
}

impl CreateForeignKeyRequest {
    /// Generate a default constraint name if not provided
    pub fn constraint_name(&self) -> String {
        self.constraint_name.clone().unwrap_or_else(|| {
            format!(
                "fk_{}_{}_{}_{}",
                self.source_table,
                self.source_column,
                self.referenced_table,
                self.referenced_column
            )
        })
    }
}
