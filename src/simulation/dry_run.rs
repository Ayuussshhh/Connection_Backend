//! Dry run executor
//!
//! Validates migrations in a transaction that gets rolled back.

use crate::error::AppError;
use crate::proposal::MigrationGenerator;
use crate::proposal::SchemaChange;
use deadpool_postgres::Pool;
use serde::Serialize;

pub struct DryRunner;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunResult {
    pub success: bool,
    pub error: Option<String>,
    pub execution_plan: Vec<String>,
    pub warnings: Vec<String>,
}

impl DryRunner {
    /// Execute a dry run of the migration
    pub async fn execute(pool: &Pool, changes: &[SchemaChange]) -> Result<DryRunResult, AppError> {
        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        
        let sql = MigrationGenerator::generate_migration(changes);
        let statements: Vec<&str> = sql.split(';').filter(|s| !s.trim().is_empty()).collect();
        
        let mut execution_plan = Vec::new();
        let mut warnings = Vec::new();
        
        for (i, statement) in statements.iter().enumerate() {
            let stmt = statement.trim();
            if stmt.is_empty() {
                continue;
            }
            
            // Get query plan
            let explain_query = format!("EXPLAIN (FORMAT TEXT) {}", stmt);
            match transaction.query(&explain_query, &[]).await {
                Ok(rows) => {
                    for row in rows {
                        let plan: String = row.get(0);
                        execution_plan.push(format!("Step {}: {}", i + 1, plan));
                    }
                }
                Err(e) => {
                    // Try to execute anyway to get actual error
                    if let Err(exec_err) = transaction.execute(stmt, &[]).await {
                        // Rollback is automatic when transaction is dropped
                        return Ok(DryRunResult {
                            success: false,
                            error: Some(format!("Statement {} failed: {}", i + 1, exec_err)),
                            execution_plan,
                            warnings,
                        });
                    }
                    warnings.push(format!("Could not get execution plan for step {}: {}", i + 1, e));
                }
            }
            
            // Actually execute the statement
            match transaction.execute(stmt, &[]).await {
                Ok(_) => {}
                Err(e) => {
                    return Ok(DryRunResult {
                        success: false,
                        error: Some(format!("Statement {} failed: {}", i + 1, e)),
                        execution_plan,
                        warnings,
                    });
                }
            }
        }
        
        // Always rollback - this is a dry run
        transaction.rollback().await?;
        
        Ok(DryRunResult {
            success: true,
            error: None,
            execution_plan,
            warnings,
        })
    }
}
