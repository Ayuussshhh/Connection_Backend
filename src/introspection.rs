//! Schema Introspection Module
//!
//! Handles introspecting database schemas from live databases.
//! This is the core of "live schema as source of truth".

use crate::error::AppError;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use tracing::debug;
use uuid::Uuid;

/// Complete schema snapshot at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaSnapshot {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub version: u64,
    pub captured_at: DateTime<Utc>,
    pub tables: Vec<Table>,
    pub foreign_keys: Vec<ForeignKey>,
    pub indexes: Vec<Index>,
    pub checksum: String,
}

impl SchemaSnapshot {
    /// Compute checksum from schema content
    pub fn compute_checksum(tables: &[Table], foreign_keys: &[ForeignKey], _indexes: &[Index]) -> String {
        let mut hasher = Sha256::new();
        
        // Hash tables in sorted order for consistency
        let mut table_strs: Vec<String> = tables.iter()
            .map(|t| format!("{}.{}", t.schema, t.name))
            .collect();
        table_strs.sort();
        
        for t in &table_strs {
            hasher.update(t.as_bytes());
        }
        
        // Hash columns
        for table in tables {
            for col in &table.columns {
                hasher.update(format!("{}.{}.{}:{}", 
                    table.schema, table.name, col.name, col.data_type).as_bytes());
            }
        }
        
        // Hash foreign keys
        for fk in foreign_keys {
            hasher.update(format!("FK:{}->{}",
                fk.constraint_name, fk.referenced_table).as_bytes());
        }
        
        let result = hasher.finalize();
        format!("{:x}", result)
    }
}

/// Table representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Table {
    pub name: String,
    pub schema: String,
    pub columns: Vec<Column>,
    pub primary_key: Option<PrimaryKey>,
    
    // Visual metadata (stored in SchemaFlow, not in DB)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default)]
    pub collapsed: bool,
    
    // Governance metadata
    #[serde(default)]
    pub governance: TableGovernance,
}

/// Column representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    pub is_unique: bool,
    pub ordinal_position: i32,
    
    // Governance
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pii_classification: Option<PiiLevel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Primary key constraint
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrimaryKey {
    pub constraint_name: String,
    pub columns: Vec<String>,
}

/// Foreign key relationship
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKey {
    pub constraint_name: String,
    pub source_schema: String,
    pub source_table: String,
    pub source_columns: Vec<String>,
    pub referenced_schema: String,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    pub on_update: String,
    pub on_delete: String,
}

/// Index representation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Index {
    pub name: String,
    pub schema: String,
    pub table: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    pub index_type: String,
}

/// Visual position on canvas
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// PII classification levels
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PiiLevel {
    None,
    Internal,       // Internal use only
    Confidential,   // PII - name, email
    Restricted,     // Sensitive PII - SSN, financial
    Secret,         // Highest sensitivity
}

/// Table governance metadata
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableGovernance {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retention_days: Option<i32>,
}

/// Schema introspector for PostgreSQL
pub struct PostgresIntrospector;

impl PostgresIntrospector {
    /// Introspect the complete schema from a PostgreSQL database
    pub async fn introspect(pool: &Pool, connection_id: Uuid) -> Result<SchemaSnapshot, AppError> {
        let client = pool.get().await?;
        
        // Get all tables
        let tables = Self::get_tables(&client).await?;
        
        // Get all foreign keys
        let foreign_keys = Self::get_foreign_keys(&client).await?;
        
        // Get all indexes
        let indexes = Self::get_indexes(&client).await?;
        
        // Compute checksum
        let checksum = SchemaSnapshot::compute_checksum(&tables, &foreign_keys, &indexes);
        
        let snapshot = SchemaSnapshot {
            id: Uuid::new_v4(),
            connection_id,
            version: 1, // Will be incremented on save
            captured_at: Utc::now(),
            tables,
            foreign_keys,
            indexes,
            checksum,
        };
        
        debug!("Introspected schema with {} tables, {} FKs, {} indexes",
            snapshot.tables.len(),
            snapshot.foreign_keys.len(),
            snapshot.indexes.len()
        );
        
        Ok(snapshot)
    }
    
    /// Get all tables with columns
    async fn get_tables(client: &deadpool_postgres::Client) -> Result<Vec<Table>, AppError> {
        // Query for tables
        let table_query = r#"
            SELECT 
                t.table_schema,
                t.table_name
            FROM information_schema.tables t
            WHERE t.table_schema NOT IN ('pg_catalog', 'information_schema')
              AND t.table_type = 'BASE TABLE'
            ORDER BY t.table_schema, t.table_name
        "#;
        
        let table_rows = client.query(table_query, &[]).await?;
        
        let mut tables = Vec::new();
        
        for row in table_rows {
            let schema: String = row.get("table_schema");
            let name: String = row.get("table_name");
            
            // Get columns for this table
            let columns = Self::get_columns(client, &schema, &name).await?;
            
            // Get primary key
            let primary_key = Self::get_primary_key(client, &schema, &name).await?;
            
            tables.push(Table {
                name,
                schema,
                columns,
                primary_key,
                position: None,
                color: None,
                collapsed: false,
                governance: TableGovernance::default(),
            });
        }
        
        Ok(tables)
    }
    
    /// Get columns for a table
    async fn get_columns(
        client: &deadpool_postgres::Client,
        schema: &str,
        table: &str,
    ) -> Result<Vec<Column>, AppError> {
        let query = r#"
            SELECT 
                c.column_name,
                c.data_type,
                c.is_nullable,
                c.column_default,
                c.ordinal_position,
                COALESCE(
                    (SELECT true FROM information_schema.table_constraints tc
                     JOIN information_schema.key_column_usage kcu 
                        ON tc.constraint_name = kcu.constraint_name
                        AND tc.table_schema = kcu.table_schema
                     WHERE tc.constraint_type = 'PRIMARY KEY'
                        AND tc.table_schema = c.table_schema
                        AND tc.table_name = c.table_name
                        AND kcu.column_name = c.column_name
                     LIMIT 1),
                    false
                ) as is_primary_key,
                COALESCE(
                    (SELECT true FROM information_schema.table_constraints tc
                     JOIN information_schema.key_column_usage kcu 
                        ON tc.constraint_name = kcu.constraint_name
                        AND tc.table_schema = kcu.table_schema
                     WHERE tc.constraint_type = 'UNIQUE'
                        AND tc.table_schema = c.table_schema
                        AND tc.table_name = c.table_name
                        AND kcu.column_name = c.column_name
                     LIMIT 1),
                    false
                ) as is_unique
            FROM information_schema.columns c
            WHERE c.table_schema = $1 AND c.table_name = $2
            ORDER BY c.ordinal_position
        "#;
        
        let rows = client.query(query, &[&schema, &table]).await?;
        
        let columns = rows.iter().map(|row| {
            Column {
                name: row.get("column_name"),
                data_type: row.get("data_type"),
                nullable: row.get::<_, String>("is_nullable") == "YES",
                default_value: row.get("column_default"),
                ordinal_position: row.get("ordinal_position"),
                is_primary_key: row.get("is_primary_key"),
                is_unique: row.get("is_unique"),
                pii_classification: None,
                description: None,
                tags: vec![],
            }
        }).collect();
        
        Ok(columns)
    }
    
    /// Get primary key for a table
    async fn get_primary_key(
        client: &deadpool_postgres::Client,
        schema: &str,
        table: &str,
    ) -> Result<Option<PrimaryKey>, AppError> {
        let query = r#"
            SELECT 
                tc.constraint_name,
                COALESCE(array_agg(kcu.column_name::text ORDER BY kcu.ordinal_position), ARRAY[]::text[]) as columns
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu 
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            WHERE tc.constraint_type = 'PRIMARY KEY'
                AND tc.table_schema = $1
                AND tc.table_name = $2
            GROUP BY tc.constraint_name
        "#;
        
        let rows = client.query(query, &[&schema, &table]).await?;
        
        if let Some(row) = rows.first() {
            let constraint_name: String = row.get("constraint_name");
            let columns: Vec<String> = row.try_get("columns").unwrap_or_default();
            Ok(Some(PrimaryKey {
                constraint_name,
                columns,
            }))
        } else {
            Ok(None)
        }
    }
    
    /// Get all foreign keys
    async fn get_foreign_keys(client: &deadpool_postgres::Client) -> Result<Vec<ForeignKey>, AppError> {
        let query = r#"
            SELECT
                tc.constraint_name,
                tc.table_schema as source_schema,
                tc.table_name as source_table,
                COALESCE(array_agg(kcu.column_name::text ORDER BY kcu.ordinal_position), ARRAY[]::text[]) as source_columns,
                ccu.table_schema as referenced_schema,
                ccu.table_name as referenced_table,
                COALESCE(array_agg(ccu.column_name::text ORDER BY kcu.ordinal_position), ARRAY[]::text[]) as referenced_columns,
                rc.update_rule as on_update,
                rc.delete_rule as on_delete
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu 
                ON tc.constraint_name = kcu.constraint_name
                AND tc.table_schema = kcu.table_schema
            JOIN information_schema.constraint_column_usage ccu 
                ON tc.constraint_name = ccu.constraint_name
                AND tc.table_schema = ccu.constraint_schema
            JOIN information_schema.referential_constraints rc
                ON tc.constraint_name = rc.constraint_name
                AND tc.table_schema = rc.constraint_schema
            WHERE tc.constraint_type = 'FOREIGN KEY'
                AND tc.table_schema NOT IN ('pg_catalog', 'information_schema')
            GROUP BY 
                tc.constraint_name,
                tc.table_schema,
                tc.table_name,
                ccu.table_schema,
                ccu.table_name,
                rc.update_rule,
                rc.delete_rule
            ORDER BY tc.table_schema, tc.table_name, tc.constraint_name
        "#;
        
        let rows = client.query(query, &[]).await?;
        
        let foreign_keys = rows.iter().map(|row| {
            ForeignKey {
                constraint_name: row.get("constraint_name"),
                source_schema: row.get("source_schema"),
                source_table: row.get("source_table"),
                source_columns: row.try_get("source_columns").unwrap_or_default(),
                referenced_schema: row.get("referenced_schema"),
                referenced_table: row.get("referenced_table"),
                referenced_columns: row.try_get("referenced_columns").unwrap_or_default(),
                on_update: row.get("on_update"),
                on_delete: row.get("on_delete"),
            }
        }).collect();
        
        Ok(foreign_keys)
    }
    
    /// Get all indexes
    async fn get_indexes(client: &deadpool_postgres::Client) -> Result<Vec<Index>, AppError> {
        let query = r#"
            SELECT
                i.relname as index_name,
                n.nspname as schema_name,
                t.relname as table_name,
                COALESCE(array_agg(a.attname::text ORDER BY array_position(ix.indkey, a.attnum)), ARRAY[]::text[]) as columns,
                ix.indisunique as is_unique,
                ix.indisprimary as is_primary,
                am.amname as index_type
            FROM pg_class t
            JOIN pg_index ix ON t.oid = ix.indrelid
            JOIN pg_class i ON i.oid = ix.indexrelid
            JOIN pg_namespace n ON n.oid = t.relnamespace
            JOIN pg_am am ON i.relam = am.oid
            JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
            WHERE n.nspname NOT IN ('pg_catalog', 'information_schema')
              AND t.relkind = 'r'
            GROUP BY i.relname, n.nspname, t.relname, ix.indisunique, ix.indisprimary, am.amname
            ORDER BY n.nspname, t.relname, i.relname
        "#;
        
        let rows = client.query(query, &[]).await?;
        
        let indexes = rows.iter().map(|row| {
            Index {
                name: row.get("index_name"),
                schema: row.get("schema_name"),
                table: row.get("table_name"),
                columns: row.try_get("columns").unwrap_or_default(),
                is_unique: row.get("is_unique"),
                is_primary: row.get("is_primary"),
                index_type: row.get("index_type"),
            }
        }).collect();
        
        Ok(indexes)
    }
}

/// Drift detection result
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DriftReport {
    pub detected_at: DateTime<Utc>,
    pub has_drift: bool,
    pub old_checksum: String,
    pub new_checksum: String,
    pub changes: Vec<DriftChange>,
}

/// Individual drift change
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct DriftChange {
    pub change_type: DriftChangeType,
    pub object_type: String,
    pub object_name: String,
    pub details: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
pub enum DriftChangeType {
    Added,
    Removed,
    Modified,
}

/// Compare two schemas and detect drift
#[allow(dead_code)]
pub fn detect_drift(old: &SchemaSnapshot, new: &SchemaSnapshot) -> DriftReport {
    let mut changes = Vec::new();
    
    // Build lookup maps
    let old_tables: HashMap<String, &Table> = old.tables.iter()
        .map(|t| (format!("{}.{}", t.schema, t.name), t))
        .collect();
    
    let new_tables: HashMap<String, &Table> = new.tables.iter()
        .map(|t| (format!("{}.{}", t.schema, t.name), t))
        .collect();
    
    // Find added tables
    for (name, _) in &new_tables {
        if !old_tables.contains_key(name) {
            changes.push(DriftChange {
                change_type: DriftChangeType::Added,
                object_type: "table".to_string(),
                object_name: name.clone(),
                details: None,
            });
        }
    }
    
    // Find removed tables
    for (name, _) in &old_tables {
        if !new_tables.contains_key(name) {
            changes.push(DriftChange {
                change_type: DriftChangeType::Removed,
                object_type: "table".to_string(),
                object_name: name.clone(),
                details: None,
            });
        }
    }
    
    // Find modified tables (column changes)
    for (name, old_table) in &old_tables {
        if let Some(new_table) = new_tables.get(name) {
            let old_cols: HashMap<&str, &Column> = old_table.columns.iter()
                .map(|c| (c.name.as_str(), c))
                .collect();
            
            let new_cols: HashMap<&str, &Column> = new_table.columns.iter()
                .map(|c| (c.name.as_str(), c))
                .collect();
            
            // Check for column changes
            for (col_name, new_col) in &new_cols {
                if let Some(old_col) = old_cols.get(col_name) {
                    if old_col.data_type != new_col.data_type 
                        || old_col.nullable != new_col.nullable {
                        changes.push(DriftChange {
                            change_type: DriftChangeType::Modified,
                            object_type: "column".to_string(),
                            object_name: format!("{}.{}", name, col_name),
                            details: Some(format!(
                                "Type: {} -> {}, Nullable: {} -> {}",
                                old_col.data_type, new_col.data_type,
                                old_col.nullable, new_col.nullable
                            )),
                        });
                    }
                } else {
                    changes.push(DriftChange {
                        change_type: DriftChangeType::Added,
                        object_type: "column".to_string(),
                        object_name: format!("{}.{}", name, col_name),
                        details: None,
                    });
                }
            }
            
            // Check for removed columns
            for col_name in old_cols.keys() {
                if !new_cols.contains_key(col_name) {
                    changes.push(DriftChange {
                        change_type: DriftChangeType::Removed,
                        object_type: "column".to_string(),
                        object_name: format!("{}.{}", name, col_name),
                        details: None,
                    });
                }
            }
        }
    }
    
    DriftReport {
        detected_at: Utc::now(),
        has_drift: !changes.is_empty(),
        old_checksum: old.checksum.clone(),
        new_checksum: new.checksum.clone(),
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_checksum_consistency() {
        let tables = vec![
            Table {
                name: "users".to_string(),
                schema: "public".to_string(),
                columns: vec![
                    Column {
                        name: "id".to_string(),
                        data_type: "integer".to_string(),
                        nullable: false,
                        default_value: None,
                        ordinal_position: 1,
                        is_primary_key: true,
                        is_unique: true,
                        pii_classification: None,
                        description: None,
                        tags: vec![],
                    }
                ],
                primary_key: None,
                position: None,
                color: None,
                collapsed: false,
                governance: TableGovernance::default(),
            }
        ];
        
        let checksum1 = SchemaSnapshot::compute_checksum(&tables, &[], &[]);
        let checksum2 = SchemaSnapshot::compute_checksum(&tables, &[], &[]);
        
        assert_eq!(checksum1, checksum2);
    }
}
