//! Schema change operations
//!
//! Provides utilities for working with schema changes.

use crate::proposal::SchemaChange;

impl SchemaChange {
    /// Get a human-readable description of the change
    pub fn description(&self) -> String {
        match self {
            SchemaChange::CreateTable(c) => {
                format!("Create table {}.{}", c.schema, c.table_name)
            }
            SchemaChange::DropTable(c) => {
                format!("Drop table {}.{}", c.schema, c.table_name)
            }
            SchemaChange::RenameTable(c) => {
                format!("Rename table {}.{} to {}", c.schema, c.old_name, c.new_name)
            }
            SchemaChange::AddColumn(c) => {
                format!("Add column {} to {}.{}", c.column.name, c.schema, c.table_name)
            }
            SchemaChange::DropColumn(c) => {
                format!("Drop column {} from {}.{}", c.column_name, c.schema, c.table_name)
            }
            SchemaChange::ModifyColumn(c) => {
                format!("Modify column {} in {}.{}", c.column_name, c.schema, c.table_name)
            }
            SchemaChange::RenameColumn(c) => {
                format!("Rename column {} to {} in {}.{}", c.old_name, c.new_name, c.schema, c.table_name)
            }
            SchemaChange::AddForeignKey(c) => {
                format!("Add foreign key from {}.{} to {}.{}", 
                    c.source_schema, c.source_table, c.target_schema, c.target_table)
            }
            SchemaChange::DropForeignKey(c) => {
                format!("Drop foreign key {} from {}.{}", c.constraint_name, c.schema, c.table_name)
            }
            SchemaChange::AddIndex(c) => {
                format!("Add {}index on {}.{}", 
                    if c.unique { "unique " } else { "" }, c.schema, c.table_name)
            }
            SchemaChange::DropIndex(c) => {
                format!("Drop index {}.{}", c.schema, c.index_name)
            }
        }
    }

    /// Get the target table for this change
    pub fn target_table(&self) -> Option<(String, String)> {
        match self {
            SchemaChange::CreateTable(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::DropTable(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::RenameTable(c) => Some((c.schema.clone(), c.old_name.clone())),
            SchemaChange::AddColumn(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::DropColumn(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::ModifyColumn(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::RenameColumn(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::AddForeignKey(c) => Some((c.source_schema.clone(), c.source_table.clone())),
            SchemaChange::DropForeignKey(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::AddIndex(c) => Some((c.schema.clone(), c.table_name.clone())),
            SchemaChange::DropIndex(c) => Some((c.schema.clone(), c.index_name.clone())),
        }
    }

    /// Check if this is a destructive change
    pub fn is_destructive(&self) -> bool {
        matches!(
            self,
            SchemaChange::DropTable(_) | SchemaChange::DropColumn(_) | SchemaChange::DropForeignKey(_) | SchemaChange::DropIndex(_)
        )
    }

    /// Check if this change requires a table lock
    pub fn requires_table_lock(&self) -> bool {
        matches!(
            self,
            SchemaChange::AddColumn(_) | SchemaChange::DropColumn(_) | SchemaChange::ModifyColumn(_) |
            SchemaChange::AddForeignKey(_) | SchemaChange::DropForeignKey(_)
        )
    }
}
