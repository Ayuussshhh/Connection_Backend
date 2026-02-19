//! Schema Diff Engine
//!
//! The core comparison engine that detects changes between schema snapshots.
//! This is the "git diff" for your database schema.

use crate::introspection::{Column, ForeignKey, Index, SchemaSnapshot, Table};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Type of schema change detected
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    /// Object was added
    Added,
    /// Object was removed
    Removed,
    /// Object was modified
    Modified,
    /// Object was renamed (heuristic detection)
    Renamed,
}

/// Categories of schema objects
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectType {
    Table,
    Column,
    Index,
    ForeignKey,
    PrimaryKey,
    Constraint,
}

/// A single item in the schema diff
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiffItem {
    pub change_type: ChangeType,
    pub object_type: ObjectType,
    /// Full path to the object (e.g., "public.users.email")
    pub object_path: String,
    /// Human-readable description
    pub description: String,
    /// Before state (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before: Option<serde_json::Value>,
    /// After state (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<serde_json::Value>,
    /// Risk level for this specific change
    pub risk_level: RiskLevel,
    /// Breaking change indicator
    pub is_breaking: bool,
}

/// Risk level classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Safe,
    Low,
    Medium,
    High,
    Critical,
}

/// Complete schema diff result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaDiff {
    /// From snapshot version
    pub from_version: u64,
    /// To snapshot version  
    pub to_version: u64,
    /// From snapshot checksum
    pub from_checksum: String,
    /// To snapshot checksum
    pub to_checksum: String,
    /// All detected changes
    pub changes: Vec<SchemaDiffItem>,
    /// Summary statistics
    pub summary: DiffSummary,
    /// Overall risk level
    pub overall_risk: RiskLevel,
    /// Has any breaking changes
    pub has_breaking_changes: bool,
}

/// Summary statistics for the diff
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffSummary {
    pub tables_added: usize,
    pub tables_removed: usize,
    pub tables_modified: usize,
    pub columns_added: usize,
    pub columns_removed: usize,
    pub columns_modified: usize,
    pub indexes_added: usize,
    pub indexes_removed: usize,
    pub fks_added: usize,
    pub fks_removed: usize,
    pub total_changes: usize,
}

/// The diff engine that compares schema snapshots
pub struct DiffEngine;

impl DiffEngine {
    /// Compare two schema snapshots and return all differences
    pub fn diff(from: &SchemaSnapshot, to: &SchemaSnapshot) -> SchemaDiff {
        let mut changes = Vec::new();
        
        // Diff tables
        Self::diff_tables(&from.tables, &to.tables, &mut changes);
        
        // Diff foreign keys
        Self::diff_foreign_keys(&from.foreign_keys, &to.foreign_keys, &mut changes);
        
        // Diff indexes
        Self::diff_indexes(&from.indexes, &to.indexes, &mut changes);
        
        // Calculate summary
        let summary = Self::calculate_summary(&changes);
        
        // Calculate overall risk
        let overall_risk = Self::calculate_overall_risk(&changes);
        let has_breaking_changes = changes.iter().any(|c| c.is_breaking);
        
        SchemaDiff {
            from_version: from.version,
            to_version: to.version,
            from_checksum: from.checksum.clone(),
            to_checksum: to.checksum.clone(),
            changes,
            summary,
            overall_risk,
            has_breaking_changes,
        }
    }

    fn diff_tables(from_tables: &[Table], to_tables: &[Table], changes: &mut Vec<SchemaDiffItem>) {
        // Build lookup maps
        let from_map: HashMap<String, &Table> = from_tables
            .iter()
            .map(|t| (format!("{}.{}", t.schema, t.name), t))
            .collect();
        
        let to_map: HashMap<String, &Table> = to_tables
            .iter()
            .map(|t| (format!("{}.{}", t.schema, t.name), t))
            .collect();
        
        let from_keys: HashSet<_> = from_map.keys().collect();
        let to_keys: HashSet<_> = to_map.keys().collect();
        
        // Detect added tables
        for key in to_keys.difference(&from_keys) {
            let table = to_map.get(*key).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Added,
                object_type: ObjectType::Table,
                object_path: key.to_string(),
                description: format!("Table {} created with {} columns", key, table.columns.len()),
                before: None,
                after: Some(serde_json::to_value(table).unwrap_or_default()),
                risk_level: RiskLevel::Safe,
                is_breaking: false,
            });
        }
        
        // Detect removed tables
        for key in from_keys.difference(&to_keys) {
            let table = from_map.get(*key).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Removed,
                object_type: ObjectType::Table,
                object_path: key.to_string(),
                description: format!("Table {} dropped ({} columns, all data lost)", key, table.columns.len()),
                before: Some(serde_json::to_value(table).unwrap_or_default()),
                after: None,
                risk_level: RiskLevel::Critical,
                is_breaking: true,
            });
        }
        
        // Detect modified tables (compare columns)
        for key in from_keys.intersection(&to_keys) {
            let from_table = from_map.get(*key).unwrap();
            let to_table = to_map.get(*key).unwrap();
            Self::diff_columns(from_table, to_table, changes);
        }
    }

    fn diff_columns(from_table: &Table, to_table: &Table, changes: &mut Vec<SchemaDiffItem>) {
        let table_path = format!("{}.{}", from_table.schema, from_table.name);
        
        let from_cols: HashMap<&str, &Column> = from_table
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c))
            .collect();
        
        let to_cols: HashMap<&str, &Column> = to_table
            .columns
            .iter()
            .map(|c| (c.name.as_str(), c))
            .collect();
        
        let from_keys: HashSet<_> = from_cols.keys().copied().collect();
        let to_keys: HashSet<_> = to_cols.keys().copied().collect();
        
        // Detect added columns
        for col_name in to_keys.difference(&from_keys) {
            let col = to_cols.get(col_name).unwrap();
            let (risk, is_breaking) = Self::assess_add_column_risk(col);
            
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Added,
                object_type: ObjectType::Column,
                object_path: format!("{}.{}", table_path, col_name),
                description: format!(
                    "Column {} added (type: {}, nullable: {})",
                    col_name, col.data_type, col.nullable
                ),
                before: None,
                after: Some(serde_json::to_value(col).unwrap_or_default()),
                risk_level: risk,
                is_breaking,
            });
        }
        
        // Detect removed columns
        for col_name in from_keys.difference(&to_keys) {
            let col = from_cols.get(col_name).unwrap();
            
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Removed,
                object_type: ObjectType::Column,
                object_path: format!("{}.{}", table_path, col_name),
                description: format!(
                    "Column {} dropped (type: {}, data lost)",
                    col_name, col.data_type
                ),
                before: Some(serde_json::to_value(col).unwrap_or_default()),
                after: None,
                risk_level: RiskLevel::High,
                is_breaking: true,
            });
        }
        
        // Detect modified columns
        for col_name in from_keys.intersection(&to_keys) {
            let from_col = from_cols.get(col_name).unwrap();
            let to_col = to_cols.get(col_name).unwrap();
            
            if let Some(change) = Self::compare_columns(&table_path, from_col, to_col) {
                changes.push(change);
            }
        }
    }

    fn compare_columns(table_path: &str, from: &Column, to: &Column) -> Option<SchemaDiffItem> {
        let mut modifications = Vec::new();
        let mut risk = RiskLevel::Low;
        let mut is_breaking = false;
        
        // Type change
        if from.data_type != to.data_type {
            modifications.push(format!("type: {} → {}", from.data_type, to.data_type));
            risk = RiskLevel::High;
            is_breaking = Self::is_type_change_breaking(&from.data_type, &to.data_type);
        }
        
        // Nullable change
        if from.nullable != to.nullable {
            if to.nullable {
                modifications.push("now nullable".to_string());
            } else {
                modifications.push("now NOT NULL".to_string());
                risk = RiskLevel::Medium;
                is_breaking = true; // Could fail if NULLs exist
            }
        }
        
        // Default value change
        if from.default_value != to.default_value {
            modifications.push(format!(
                "default: {:?} → {:?}",
                from.default_value, to.default_value
            ));
        }
        
        // Primary key change
        if from.is_primary_key != to.is_primary_key {
            if to.is_primary_key {
                modifications.push("added to PRIMARY KEY".to_string());
                risk = RiskLevel::High;
            } else {
                modifications.push("removed from PRIMARY KEY".to_string());
                risk = RiskLevel::Critical;
                is_breaking = true;
            }
        }
        
        if modifications.is_empty() {
            return None;
        }
        
        Some(SchemaDiffItem {
            change_type: ChangeType::Modified,
            object_type: ObjectType::Column,
            object_path: format!("{}.{}", table_path, from.name),
            description: format!("Column {} modified: {}", from.name, modifications.join(", ")),
            before: Some(serde_json::to_value(from).unwrap_or_default()),
            after: Some(serde_json::to_value(to).unwrap_or_default()),
            risk_level: risk,
            is_breaking,
        })
    }

    fn diff_foreign_keys(from_fks: &[ForeignKey], to_fks: &[ForeignKey], changes: &mut Vec<SchemaDiffItem>) {
        let from_map: HashMap<&str, &ForeignKey> = from_fks
            .iter()
            .map(|fk| (fk.constraint_name.as_str(), fk))
            .collect();
        
        let to_map: HashMap<&str, &ForeignKey> = to_fks
            .iter()
            .map(|fk| (fk.constraint_name.as_str(), fk))
            .collect();
        
        let from_keys: HashSet<_> = from_map.keys().copied().collect();
        let to_keys: HashSet<_> = to_map.keys().copied().collect();
        
        // Added FKs
        for name in to_keys.difference(&from_keys) {
            let fk = to_map.get(name).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Added,
                object_type: ObjectType::ForeignKey,
                object_path: format!("{}.{}.{}", fk.source_schema, fk.source_table, name),
                description: format!(
                    "FK {} added: {}.{} → {}.{}",
                    name, fk.source_table, fk.source_columns.join(","),
                    fk.referenced_table, fk.referenced_columns.join(",")
                ),
                before: None,
                after: Some(serde_json::to_value(fk).unwrap_or_default()),
                risk_level: RiskLevel::Low,
                is_breaking: false,
            });
        }
        
        // Removed FKs
        for name in from_keys.difference(&to_keys) {
            let fk = from_map.get(name).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Removed,
                object_type: ObjectType::ForeignKey,
                object_path: format!("{}.{}.{}", fk.source_schema, fk.source_table, name),
                description: format!(
                    "FK {} dropped (referential integrity removed)",
                    name
                ),
                before: Some(serde_json::to_value(fk).unwrap_or_default()),
                after: None,
                risk_level: RiskLevel::Medium,
                is_breaking: false,
            });
        }
    }

    fn diff_indexes(from_idxs: &[Index], to_idxs: &[Index], changes: &mut Vec<SchemaDiffItem>) {
        let from_map: HashMap<&str, &Index> = from_idxs
            .iter()
            .map(|idx| (idx.name.as_str(), idx))
            .collect();
        
        let to_map: HashMap<&str, &Index> = to_idxs
            .iter()
            .map(|idx| (idx.name.as_str(), idx))
            .collect();
        
        let from_keys: HashSet<_> = from_map.keys().copied().collect();
        let to_keys: HashSet<_> = to_map.keys().copied().collect();
        
        // Added indexes
        for name in to_keys.difference(&from_keys) {
            let idx = to_map.get(name).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Added,
                object_type: ObjectType::Index,
                object_path: format!("{}.{}", idx.schema, name),
                description: format!(
                    "{}Index {} added on {}.{} (columns: {})",
                    if idx.is_unique { "Unique " } else { "" },
                    name, idx.schema, idx.table, idx.columns.join(", ")
                ),
                before: None,
                after: Some(serde_json::to_value(idx).unwrap_or_default()),
                risk_level: RiskLevel::Safe,
                is_breaking: false,
            });
        }
        
        // Removed indexes
        for name in from_keys.difference(&to_keys) {
            let idx = from_map.get(name).unwrap();
            changes.push(SchemaDiffItem {
                change_type: ChangeType::Removed,
                object_type: ObjectType::Index,
                object_path: format!("{}.{}", idx.schema, name),
                description: format!(
                    "Index {} dropped from {}.{} (may impact query performance)",
                    name, idx.schema, idx.table
                ),
                before: Some(serde_json::to_value(idx).unwrap_or_default()),
                after: None,
                risk_level: if idx.is_unique { RiskLevel::High } else { RiskLevel::Medium },
                is_breaking: idx.is_unique, // Unique index removal can break constraints
            });
        }
    }

    fn assess_add_column_risk(col: &Column) -> (RiskLevel, bool) {
        // NOT NULL without default is dangerous
        if !col.nullable && col.default_value.is_none() {
            return (RiskLevel::High, true);
        }
        (RiskLevel::Safe, false)
    }

    fn is_type_change_breaking(from: &str, to: &str) -> bool {
        let from_lower = from.to_lowercase();
        let to_lower = to.to_lowercase();
        
        // Widening conversions are generally safe
        let safe_widenings = [
            ("integer", "bigint"),
            ("smallint", "integer"),
            ("real", "double precision"),
            ("varchar", "text"),
            ("char", "varchar"),
        ];
        
        for (f, t) in safe_widenings {
            if from_lower.contains(f) && to_lower.contains(t) {
                return false;
            }
        }
        
        // Any other type change is potentially breaking
        true
    }

    fn calculate_summary(changes: &[SchemaDiffItem]) -> DiffSummary {
        let mut summary = DiffSummary {
            tables_added: 0,
            tables_removed: 0,
            tables_modified: 0,
            columns_added: 0,
            columns_removed: 0,
            columns_modified: 0,
            indexes_added: 0,
            indexes_removed: 0,
            fks_added: 0,
            fks_removed: 0,
            total_changes: changes.len(),
        };
        
        let mut modified_tables: HashSet<String> = HashSet::new();
        
        for change in changes {
            match (change.object_type, change.change_type) {
                (ObjectType::Table, ChangeType::Added) => summary.tables_added += 1,
                (ObjectType::Table, ChangeType::Removed) => summary.tables_removed += 1,
                
                (ObjectType::Column, ChangeType::Added) => {
                    summary.columns_added += 1;
                    if let Some(table) = change.object_path.rsplit('.').nth(1) {
                        modified_tables.insert(table.to_string());
                    }
                }
                (ObjectType::Column, ChangeType::Removed) => {
                    summary.columns_removed += 1;
                    if let Some(table) = change.object_path.rsplit('.').nth(1) {
                        modified_tables.insert(table.to_string());
                    }
                }
                (ObjectType::Column, ChangeType::Modified) => {
                    summary.columns_modified += 1;
                    if let Some(table) = change.object_path.rsplit('.').nth(1) {
                        modified_tables.insert(table.to_string());
                    }
                }
                
                (ObjectType::Index, ChangeType::Added) => summary.indexes_added += 1,
                (ObjectType::Index, ChangeType::Removed) => summary.indexes_removed += 1,
                
                (ObjectType::ForeignKey, ChangeType::Added) => summary.fks_added += 1,
                (ObjectType::ForeignKey, ChangeType::Removed) => summary.fks_removed += 1,
                
                _ => {}
            }
        }
        
        summary.tables_modified = modified_tables.len();
        summary
    }

    fn calculate_overall_risk(changes: &[SchemaDiffItem]) -> RiskLevel {
        let max_risk = changes
            .iter()
            .map(|c| c.risk_level)
            .max_by(|a, b| {
                let order = |r: &RiskLevel| match r {
                    RiskLevel::Safe => 0,
                    RiskLevel::Low => 1,
                    RiskLevel::Medium => 2,
                    RiskLevel::High => 3,
                    RiskLevel::Critical => 4,
                };
                order(a).cmp(&order(b))
            });
        
        max_risk.unwrap_or(RiskLevel::Safe)
    }
}
