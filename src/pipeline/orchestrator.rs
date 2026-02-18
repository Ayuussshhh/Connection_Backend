//! Orchestrator - Safe execution of schema migrations

use crate::error::AppError;
use crate::pipeline::proposal::{MigrationArtifacts, SchemaProposal};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Orchestrator for safely executing schema migrations
pub struct Orchestrator;

impl Orchestrator {
    pub fn new() -> Self {
        Self
    }

    /// Execute a migration against the database
    pub async fn execute(
        &self,
        _proposal: &SchemaProposal,
        _dry_run: bool,
    ) -> Result<ExecutionResult, AppError> {
        // In a real implementation, this would:
        // 1. Start a transaction
        // 2. Execute each statement in the migration
        // 3. Record the execution in audit log
        // 4. Commit or rollback based on success
        
        Ok(ExecutionResult {
            id: Uuid::new_v4(),
            proposal_id: _proposal.id,
            success: true,
            dry_run: _dry_run,
            executed_statements: _proposal.migration
                .as_ref()
                .map(|m| vec![m.up_sql.clone()])
                .unwrap_or_default(),
            error: None,
            duration_ms: 100,
            executed_at: Utc::now(),
        })
    }

    /// Rollback a previously executed migration
    pub async fn rollback(
        &self,
        _proposal: &SchemaProposal,
    ) -> Result<ExecutionResult, AppError> {
        Ok(ExecutionResult {
            id: Uuid::new_v4(),
            proposal_id: _proposal.id,
            success: true,
            dry_run: false,
            executed_statements: _proposal.migration
                .as_ref()
                .map(|m| vec![m.down_sql.clone()])
                .unwrap_or_default(),
            error: None,
            duration_ms: 50,
            executed_at: Utc::now(),
        })
    }

    /// Generate migration SQL from a proposal
    pub fn generate_migration(&self, proposal: &SchemaProposal) -> MigrationArtifacts {
        use crate::pipeline::types::SchemaChange;
        
        let mut up_statements = Vec::new();
        let mut down_statements = Vec::new();

        for change in &proposal.changes {
            match change {
                SchemaChange::CreateTable { table_name, columns } => {
                    let cols: Vec<String> = columns.iter().map(|c| {
                        let mut def = format!("{} {}", c.name, c.data_type);
                        if !c.nullable {
                            def.push_str(" NOT NULL");
                        }
                        if let Some(default) = &c.default_value {
                            def.push_str(&format!(" DEFAULT {}", default));
                        }
                        if c.is_primary_key {
                            def.push_str(" PRIMARY KEY");
                        }
                        def
                    }).collect();
                    up_statements.push(format!("CREATE TABLE {} (\n  {}\n);", table_name, cols.join(",\n  ")));
                    down_statements.push(format!("DROP TABLE IF EXISTS {};", table_name));
                }
                SchemaChange::DropTable { table_name } => {
                    up_statements.push(format!("DROP TABLE {};", table_name));
                    down_statements.push(format!("-- Cannot auto-rollback DROP TABLE {}", table_name));
                }
                SchemaChange::AddColumn { table_name, column } => {
                    let mut def = format!("{} {}", column.name, column.data_type);
                    if !column.nullable {
                        def.push_str(" NOT NULL");
                    }
                    if let Some(default) = &column.default_value {
                        def.push_str(&format!(" DEFAULT {}", default));
                    }
                    up_statements.push(format!("ALTER TABLE {} ADD COLUMN {};", table_name, def));
                    down_statements.push(format!("ALTER TABLE {} DROP COLUMN {};", table_name, column.name));
                }
                SchemaChange::DropColumn { table_name, column_name } => {
                    up_statements.push(format!("ALTER TABLE {} DROP COLUMN {};", table_name, column_name));
                    down_statements.push(format!("-- Cannot auto-rollback DROP COLUMN {}.{}", table_name, column_name));
                }
                SchemaChange::RenameTable { old_name, new_name } => {
                    up_statements.push(format!("ALTER TABLE {} RENAME TO {};", old_name, new_name));
                    down_statements.push(format!("ALTER TABLE {} RENAME TO {};", new_name, old_name));
                }
                SchemaChange::RenameColumn { table_name, old_name, new_name } => {
                    up_statements.push(format!("ALTER TABLE {} RENAME COLUMN {} TO {};", table_name, old_name, new_name));
                    down_statements.push(format!("ALTER TABLE {} RENAME COLUMN {} TO {};", table_name, new_name, old_name));
                }
                SchemaChange::AddIndex { table_name, index_name, columns, unique } => {
                    let unique_str = if *unique { "UNIQUE " } else { "" };
                    up_statements.push(format!("CREATE {}INDEX {} ON {} ({});", unique_str, index_name, table_name, columns.join(", ")));
                    down_statements.push(format!("DROP INDEX IF EXISTS {};", index_name));
                }
                SchemaChange::DropIndex { index_name } => {
                    up_statements.push(format!("DROP INDEX {};", index_name));
                    down_statements.push(format!("-- Cannot auto-rollback DROP INDEX {}", index_name));
                }
                SchemaChange::AddForeignKey { table_name, constraint_name, columns, ref_table, ref_columns } => {
                    up_statements.push(format!(
                        "ALTER TABLE {} ADD CONSTRAINT {} FOREIGN KEY ({}) REFERENCES {} ({});",
                        table_name, constraint_name, columns.join(", "), ref_table, ref_columns.join(", ")
                    ));
                    down_statements.push(format!("ALTER TABLE {} DROP CONSTRAINT {};", table_name, constraint_name));
                }
                SchemaChange::DropForeignKey { table_name, constraint_name } => {
                    up_statements.push(format!("ALTER TABLE {} DROP CONSTRAINT {};", table_name, constraint_name));
                    down_statements.push(format!("-- Cannot auto-rollback DROP CONSTRAINT {}.{}", table_name, constraint_name));
                }
                _ => {}
            }
        }

        MigrationArtifacts {
            up_sql: up_statements.join("\n\n"),
            down_sql: down_statements.into_iter().rev().collect::<Vec<_>>().join("\n\n"),
            generated_at: Utc::now(),
        }
    }
}

impl Default for Orchestrator {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of executing a migration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub id: Uuid,
    pub proposal_id: Uuid,
    pub success: bool,
    pub dry_run: bool,
    pub executed_statements: Vec<String>,
    pub error: Option<String>,
    pub duration_ms: u64,
    pub executed_at: DateTime<Utc>,
}
