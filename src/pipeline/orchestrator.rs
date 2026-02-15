//! Stage Four: The Orchestrator (Safe Execution)
//!
//! Handles the "scary part" - actually executing approved migrations.
//! Features:
//! - Transaction wrapping
//! - Step-by-step execution with checkpoints
//! - Automatic rollback on failure
//! - Execution logging and audit trail

use crate::error::AppError;
use crate::pipeline::proposal::{MigrationArtifacts, MigrationStatement, SchemaProposal};
use crate::pipeline::types::ExecutionResult;
use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio_postgres::Transaction;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

// =============================================================================
// ORCHESTRATOR - Safe Execution Engine
// =============================================================================

/// The Orchestrator - executes migrations safely with rollback capability
pub struct Orchestrator;

impl Orchestrator {
    /// Execute a proposal with full safety measures
    pub async fn execute(
        pool: &Pool,
        proposal: &mut SchemaProposal,
        executor_id: Uuid,
        dry_run: bool,
    ) -> Result<ExecutionResult, AppError> {
        info!(
            "🎭 Orchestrator: {} proposal '{}' (id: {})",
            if dry_run { "Dry-running" } else { "Executing" },
            proposal.title,
            proposal.id
        );
        
        // Validate proposal is ready for execution
        Self::validate_execution_ready(proposal)?;
        
        let migration = proposal.migration.as_ref()
            .ok_or_else(|| AppError::Validation("Proposal has no generated migration".to_string()))?;
        
        // Get connection
        let mut client = pool.get().await?;
        let start_time = Instant::now();
        
        // Pre-flight check: verify base schema hasn't drifted
        let drift_check = Self::check_pre_flight(&client, proposal).await?;
        if drift_check.has_drift {
            return Err(AppError::Validation(format!(
                "Schema has drifted since proposal was created. Please refresh and re-analyze. \
                 Base checksum: {}, Current checksum: {}",
                proposal.base_checksum, drift_check.current_checksum
            )));
        }
        
        if dry_run {
            // Dry run: validate all SQL without executing
            return Self::dry_run_execution(&client, migration).await;
        }
        
        // Real execution: wrap in transaction
        let transaction = client.transaction().await?;
        
        let result = Self::execute_migration(&transaction, migration, executor_id).await;
        
        match result {
            Ok(execution_result) => {
                // Commit transaction
                transaction.commit().await?;
                
                info!(
                    "✅ Migration executed successfully in {}ms",
                    start_time.elapsed().as_millis()
                );
                
                Ok(execution_result)
            }
            Err(e) => {
                // Transaction will be rolled back automatically on drop
                error!("❌ Migration failed, rolling back: {}", e);
                
                Ok(ExecutionResult {
                    success: false,
                    executed_at: Utc::now(),
                    executed_by: executor_id,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    error: Some(e.to_string()),
                    sql_executed: migration.forward_sql.clone(),
                    rollback_sql: Some(migration.rollback_sql.clone()),
                    was_rolled_back: true,
                })
            }
        }
    }
    
    /// Execute manual rollback for a previously executed proposal
    pub async fn rollback(
        pool: &Pool,
        proposal: &SchemaProposal,
        executor_id: Uuid,
    ) -> Result<ExecutionResult, AppError> {
        info!(
            "⏪ Orchestrator: Rolling back proposal '{}' (id: {})",
            proposal.title,
            proposal.id
        );
        
        // Validate proposal was executed
        let prev_execution = proposal.execution_result.as_ref()
            .ok_or_else(|| AppError::Validation("Proposal was never executed, cannot rollback".to_string()))?;
        
        if prev_execution.was_rolled_back {
            return Err(AppError::Validation("Proposal was already rolled back".to_string()));
        }
        
        let migration = proposal.migration.as_ref()
            .ok_or_else(|| AppError::Validation("Proposal has no migration to rollback".to_string()))?;
        
        if migration.rollback_sql.is_empty() {
            return Err(AppError::Validation("No rollback SQL available for this migration".to_string()));
        }
        
        // Get connection and execute rollback
        let mut client = pool.get().await?;
        let transaction = client.transaction().await?;
        let start_time = Instant::now();
        
        // Execute rollback SQL
        let result = transaction.batch_execute(&migration.rollback_sql).await;
        
        match result {
            Ok(_) => {
                transaction.commit().await?;
                
                info!(
                    "✅ Rollback executed successfully in {}ms",
                    start_time.elapsed().as_millis()
                );
                
                Ok(ExecutionResult {
                    success: true,
                    executed_at: Utc::now(),
                    executed_by: executor_id,
                    duration_ms: start_time.elapsed().as_millis() as u64,
                    error: None,
                    sql_executed: migration.rollback_sql.clone(),
                    rollback_sql: None,
                    was_rolled_back: true,
                })
            }
            Err(e) => {
                error!("❌ Rollback failed: {}", e);
                Err(AppError::Internal(format!("Rollback failed: {}", e)))
            }
        }
    }
    
    /// Validate proposal is ready for execution
    fn validate_execution_ready(proposal: &SchemaProposal) -> Result<(), AppError> {
        if !proposal.status.can_execute() {
            return Err(AppError::Validation(format!(
                "Proposal status {:?} is not eligible for execution. Must be 'Approved'.",
                proposal.status
            )));
        }
        
        if proposal.changes.is_empty() {
            return Err(AppError::Validation("Proposal has no changes to execute".to_string()));
        }
        
        // Check risk analysis
        if let Some(ref risk) = proposal.risk_analysis {
            if risk.safety_score < 10.0 {
                return Err(AppError::Validation(format!(
                    "Proposal risk score ({:.1}) is too low for automatic execution. Manual intervention required.",
                    risk.safety_score
                )));
            }
        }
        
        Ok(())
    }
    
    /// Pre-flight check: verify schema hasn't drifted
    async fn check_pre_flight(
        client: &deadpool_postgres::Client,
        proposal: &SchemaProposal,
    ) -> Result<PreFlightResult, AppError> {
        // Compute current schema checksum
        let current_checksum = Self::compute_current_checksum(client).await?;
        
        Ok(PreFlightResult {
            has_drift: current_checksum != proposal.base_checksum,
            base_checksum: proposal.base_checksum.clone(),
            current_checksum,
        })
    }
    
    /// Compute checksum of current schema
    async fn compute_current_checksum(
        client: &deadpool_postgres::Client,
    ) -> Result<String, AppError> {
        use sha2::{Digest, Sha256};
        
        // Quick checksum based on table/column metadata
        let query = r#"
            SELECT 
                table_schema || '.' || table_name || ':' || 
                string_agg(column_name || ':' || data_type || ':' || is_nullable, ',' ORDER BY ordinal_position)
            FROM information_schema.columns
            WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
            GROUP BY table_schema, table_name
            ORDER BY table_schema, table_name
        "#;
        
        let rows = client.query(query, &[]).await?;
        
        let mut hasher = Sha256::new();
        for row in rows {
            let data: String = row.get(0);
            hasher.update(data.as_bytes());
        }
        
        Ok(format!("{:x}", hasher.finalize()))
    }
    
    /// Dry run: validate SQL without executing
    async fn dry_run_execution(
        client: &deadpool_postgres::Client,
        migration: &MigrationArtifacts,
    ) -> Result<ExecutionResult, AppError> {
        info!("🔍 Running dry-run validation...");
        
        let mut errors = Vec::new();
        let start = Instant::now();
        
        for statement in &migration.statements {
            // Use EXPLAIN to validate without executing
            let explain_sql = format!("EXPLAIN {}", statement.sql.trim_end_matches(';'));
            
            match client.query(&explain_sql, &[]).await {
                Ok(_) => {
                    debug!("✓ Statement {} validated: {}", statement.index, statement.description);
                }
                Err(_) => {
                    // Some statements can't be EXPLAINed (like CREATE TABLE)
                    // For these, we do basic syntax validation via PREPARE
                    let prepare_sql = format!(
                        "PREPARE dry_run_stmt_{} AS {}",
                        statement.index,
                        statement.sql.trim_end_matches(';')
                    );
                    
                    match client.batch_execute(&prepare_sql).await {
                        Ok(_) => {
                            // Cleanup prepared statement
                            let _ = client.batch_execute(&format!(
                                "DEALLOCATE dry_run_stmt_{}",
                                statement.index
                            )).await;
                            debug!("✓ Statement {} prepared successfully: {}", statement.index, statement.description);
                        }
                        Err(prep_err) => {
                            warn!(
                                "⚠ Statement {} validation failed: {} - Error: {}",
                                statement.index, statement.description, prep_err
                            );
                            errors.push(format!(
                                "Statement {}: {} - {}",
                                statement.index, statement.description, prep_err
                            ));
                        }
                    }
                }
            }
        }
        
        let success = errors.is_empty();
        let error_msg = if errors.is_empty() {
            None
        } else {
            Some(errors.join("\n"))
        };
        
        Ok(ExecutionResult {
            success,
            executed_at: Utc::now(),
            executed_by: Uuid::nil(), // Dry run has no executor
            duration_ms: start.elapsed().as_millis() as u64,
            error: error_msg,
            sql_executed: format!("-- DRY RUN --\n{}", migration.forward_sql),
            rollback_sql: None,
            was_rolled_back: false,
        })
    }
    
    /// Execute migration within a transaction
    async fn execute_migration(
        transaction: &Transaction<'_>,
        migration: &MigrationArtifacts,
        executor_id: Uuid,
    ) -> Result<ExecutionResult, AppError> {
        let start = Instant::now();
        let mut executed_statements = Vec::new();
        
        for statement in &migration.statements {
            info!(
                "📝 Executing statement {}/{}: {}",
                statement.index + 1,
                migration.statements.len(),
                statement.description
            );
            
            match transaction.batch_execute(&statement.sql).await {
                Ok(_) => {
                    executed_statements.push(statement.clone());
                    debug!("✓ Statement {} completed", statement.index);
                }
                Err(e) => {
                    error!(
                        "❌ Statement {} failed: {} - Error: {}",
                        statement.index, statement.description, e
                    );
                    return Err(AppError::Internal(format!(
                        "Statement '{}' failed: {}",
                        statement.description, e
                    )));
                }
            }
        }
        
        Ok(ExecutionResult {
            success: true,
            executed_at: Utc::now(),
            executed_by: executor_id,
            duration_ms: start.elapsed().as_millis() as u64,
            error: None,
            sql_executed: migration.forward_sql.clone(),
            rollback_sql: Some(migration.rollback_sql.clone()),
            was_rolled_back: false,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreFlightResult {
    pub has_drift: bool,
    pub base_checksum: String,
    pub current_checksum: String,
}

// =============================================================================
// EXECUTION PLAN - For staged/batched execution
// =============================================================================

/// Execution plan for complex migrations that need staging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ExecutionPlan {
    pub id: Uuid,
    pub proposal_id: Uuid,
    pub stages: Vec<ExecutionStage>,
    pub current_stage: usize,
    pub status: ExecutionPlanStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ExecutionStage {
    pub index: usize,
    pub name: String,
    pub statements: Vec<MigrationStatement>,
    pub status: StageStatus,
    pub can_run_concurrently: bool,
    pub require_confirmation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum ExecutionPlanStatus {
    Pending,
    InProgress,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[allow(dead_code)]
pub enum StageStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

#[allow(dead_code)]
impl ExecutionPlan {
    /// Create a new execution plan from a migration
    pub fn from_migration(proposal_id: Uuid, migration: &MigrationArtifacts) -> Self {
        let mut stages = Vec::new();
        
        // Group statements into stages
        let mut current_stage_statements = Vec::new();
        let mut stage_index = 0;
        
        for statement in &migration.statements {
            // Start a new stage for destructive operations
            if statement.is_destructive && !current_stage_statements.is_empty() {
                stages.push(ExecutionStage {
                    index: stage_index,
                    name: format!("Stage {}: Non-destructive changes", stage_index + 1),
                    statements: std::mem::take(&mut current_stage_statements),
                    status: StageStatus::Pending,
                    can_run_concurrently: false,
                    require_confirmation: false,
                    executed_at: None,
                    error: None,
                });
                stage_index += 1;
            }
            
            current_stage_statements.push(statement.clone());
            
            // Destructive operations get their own stage
            if statement.is_destructive {
                stages.push(ExecutionStage {
                    index: stage_index,
                    name: format!("Stage {}: {} (DESTRUCTIVE)", stage_index + 1, statement.description),
                    statements: std::mem::take(&mut current_stage_statements),
                    status: StageStatus::Pending,
                    can_run_concurrently: false,
                    require_confirmation: true, // Requires manual confirmation
                    executed_at: None,
                    error: None,
                });
                stage_index += 1;
            }
        }
        
        // Add remaining statements as final stage
        if !current_stage_statements.is_empty() {
            stages.push(ExecutionStage {
                index: stage_index,
                name: format!("Stage {}: Final changes", stage_index + 1),
                statements: current_stage_statements,
                status: StageStatus::Pending,
                can_run_concurrently: false,
                require_confirmation: false,
                executed_at: None,
                error: None,
            });
        }
        
        Self {
            id: Uuid::new_v4(),
            proposal_id,
            stages,
            current_stage: 0,
            status: ExecutionPlanStatus::Pending,
            created_at: Utc::now(),
        }
    }
}
