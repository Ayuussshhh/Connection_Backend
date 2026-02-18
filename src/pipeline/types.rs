//! Schema types for the governance pipeline

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Schema change types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchemaChange {
    CreateTable {
        table_name: String,
        columns: Vec<ColumnDef>,
    },
    DropTable {
        table_name: String,
    },
    AddColumn {
        table_name: String,
        column: ColumnDef,
    },
    DropColumn {
        table_name: String,
        column_name: String,
    },
    AlterColumn {
        table_name: String,
        column_name: String,
        new_type: Option<String>,
        new_nullable: Option<bool>,
        new_default: Option<String>,
    },
    RenameTable {
        old_name: String,
        new_name: String,
    },
    RenameColumn {
        table_name: String,
        old_name: String,
        new_name: String,
    },
    AddIndex {
        table_name: String,
        index_name: String,
        columns: Vec<String>,
        unique: bool,
    },
    DropIndex {
        index_name: String,
    },
    AddForeignKey {
        table_name: String,
        constraint_name: String,
        columns: Vec<String>,
        ref_table: String,
        ref_columns: Vec<String>,
    },
    DropForeignKey {
        table_name: String,
        constraint_name: String,
    },
    AddCheck {
        table_name: String,
        constraint_name: String,
        expression: String,
    },
    AddUnique {
        table_name: String,
        constraint_name: String,
        columns: Vec<String>,
    },
}

/// Column definition
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDef {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
}

/// Comment target for proposal comments
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentTarget {
    Proposal,
    Change { index: usize },
}
