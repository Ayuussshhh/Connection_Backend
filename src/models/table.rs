//! Table-related models and DTOs

use serde::{Deserialize, Serialize};
use validator::Validate;

/// Column definition for table creation
#[derive(Debug, Deserialize, Serialize, Clone, Validate)]
pub struct ColumnDefinition {
    #[validate(length(min = 1, max = 63, message = "Column name must be between 1 and 63 characters"))]
    pub name: String,
    
    #[validate(length(min = 1, message = "Column type is required"))]
    #[serde(rename = "type")]
    pub data_type: String,
    
    #[serde(default)]
    pub nullable: Option<bool>,
    
    #[serde(default)]
    pub primary_key: Option<bool>,
    
    #[serde(default)]
    pub unique: Option<bool>,
    
    #[serde(default)]
    pub default_value: Option<String>,
}

/// Request to create a new table
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableRequest {
    #[validate(length(min = 1, max = 63, message = "Table name must be between 1 and 63 characters"))]
    pub table_name: String,
    
    #[validate(length(min = 1, message = "At least one column is required"))]
    #[validate(nested)]
    pub columns: Vec<ColumnDefinition>,
}

/// Request to get columns for a table
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListColumnsQuery {
    pub table_name: String,
}

/// Response containing list of tables
#[derive(Debug, Serialize)]
pub struct TableListResponse {
    pub tables: Vec<TableInfo>,
}

/// Table information
#[derive(Debug, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub schema: String,
    pub owner: String,
    #[serde(rename = "type")]
    pub table_type: String,
}

/// Column information
#[derive(Debug, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    #[serde(rename = "type")]
    pub data_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<i32>,
    pub is_primary_key: bool,
    pub is_unique: bool,
}

/// Response containing list of columns
#[derive(Debug, Serialize)]
pub struct ColumnListResponse {
    pub columns: Vec<ColumnInfo>,
}

/// Valid PostgreSQL data types for validation
pub const VALID_DATA_TYPES: &[&str] = &[
    // Numeric types
    "smallint", "integer", "bigint", "decimal", "numeric", "real", "double precision",
    "smallserial", "serial", "bigserial",
    // Character types
    "character", "char", "character varying", "varchar", "text",
    // Binary types
    "bytea",
    // Date/time types
    "timestamp", "timestamp with time zone", "timestamp without time zone",
    "date", "time", "time with time zone", "time without time zone", "interval",
    // Boolean
    "boolean", "bool",
    // UUID
    "uuid",
    // JSON types
    "json", "jsonb",
    // Array types (simplified)
    "integer[]", "text[]", "varchar[]",
    // Other common types
    "inet", "cidr", "macaddr",
    // Custom types (allow for user-defined types)
    "int", "int2", "int4", "int8", "float4", "float8",
];

impl ColumnDefinition {
    /// Validates the column data type
    pub fn validate_data_type(&self) -> Result<(), String> {
        let normalized = self.data_type.to_lowercase();
        let base_type = normalized.split('(').next().unwrap_or(&normalized).trim();
        
        // Check if it's a valid type or starts with a valid type
        let is_valid = VALID_DATA_TYPES.iter().any(|t| {
            base_type == *t || base_type.starts_with(t)
        }) || base_type.ends_with("[]"); // Allow array types
        
        if is_valid {
            Ok(())
        } else {
            Err(format!("Invalid data type: {}", self.data_type))
        }
    }

    /// Generates SQL column definition
    pub fn to_sql(&self) -> String {
        let mut parts = vec![format!("\"{}\" {}", self.name, self.data_type)];
        
        if let Some(false) = self.nullable {
            parts.push("NOT NULL".to_string());
        }
        
        if let Some(true) = self.primary_key {
            parts.push("PRIMARY KEY".to_string());
        }
        
        if let Some(true) = self.unique {
            parts.push("UNIQUE".to_string());
        }
        
        if let Some(ref default) = self.default_value {
            parts.push(format!("DEFAULT {}", default));
        }
        
        parts.join(" ")
    }
}
