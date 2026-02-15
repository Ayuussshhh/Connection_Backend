//! Stage Two: The Proposal (The "Glow" Layer)
//!
//! Instead of direct edits, users create "Draft Objects" (proposals).
//! This enables:
//! - Multi-user collaboration
//! - Review workflows
//! - Audit trails
//! - Safe experimentation before execution

use crate::error::AppError;
use crate::pipeline::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// SCHEMA PROPOSAL - Like a GitHub PR for your database
// =============================================================================

/// A proposed schema change (like a GitHub Pull Request)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProposal {
    pub id: Uuid,
    pub title: String,
    pub description: String,
    
    /// Author information
    pub author_id: Uuid,
    pub author_name: String,
    
    /// Current status
    pub status: ProposalStatus,
    
    /// Connection this proposal is for
    pub connection_id: Uuid,
    
    /// Base schema snapshot (before changes)
    pub base_snapshot_id: Uuid,
    pub base_checksum: String,
    
    /// The proposed changes
    pub changes: Vec<SchemaChange>,
    
    /// Generated migration artifacts
    pub migration: Option<MigrationArtifacts>,
    
    /// Risk analysis results
    pub risk_analysis: Option<RiskAnalysis>,
    
    /// Review workflow
    pub reviewers: Vec<Uuid>,
    pub approvals: Vec<Approval>,
    pub rejections: Vec<Rejection>,
    pub comments: Vec<Comment>,
    
    /// Execution history
    pub execution_result: Option<ExecutionResult>,
    
    /// Timestamps
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submitted_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merged_at: Option<DateTime<Utc>>,
}

impl SchemaProposal {
    /// Create a new draft proposal
    pub fn new(
        connection_id: Uuid,
        base_snapshot_id: Uuid,
        base_checksum: String,
        author_id: Uuid,
        author_name: String,
        title: String,
        description: String,
    ) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            title,
            description,
            author_id,
            author_name,
            status: ProposalStatus::Draft,
            connection_id,
            base_snapshot_id,
            base_checksum,
            changes: Vec::new(),
            migration: None,
            risk_analysis: None,
            reviewers: Vec::new(),
            approvals: Vec::new(),
            rejections: Vec::new(),
            comments: Vec::new(),
            execution_result: None,
            created_at: now,
            updated_at: now,
            submitted_at: None,
            merged_at: None,
        }
    }
    
    /// Add a change to the proposal
    pub fn add_change(&mut self, change: SchemaChange) {
        self.changes.push(change);
        self.updated_at = Utc::now();
        // Invalidate cached migration
        self.migration = None;
        self.risk_analysis = None;
    }
    
    /// Remove a change by index
    #[allow(dead_code)]
    pub fn remove_change(&mut self, index: usize) -> Option<SchemaChange> {
        if index < self.changes.len() {
            self.updated_at = Utc::now();
            self.migration = None;
            self.risk_analysis = None;
            Some(self.changes.remove(index))
        } else {
            None
        }
    }
    
    /// Get count of schema-modifying changes (excluding governance)
    #[allow(dead_code)]
    pub fn schema_change_count(&self) -> usize {
        self.changes.iter().filter(|c| c.modifies_database()).count()
    }
    
    /// Submit for review
    pub fn submit_for_review(&mut self) -> Result<(), AppError> {
        if !self.status.can_submit_for_review() {
            return Err(AppError::Validation(format!(
                "Cannot submit proposal with status {:?} for review",
                self.status
            )));
        }
        if self.changes.is_empty() {
            return Err(AppError::Validation("Cannot submit empty proposal".to_string()));
        }
        self.status = ProposalStatus::Open;
        self.submitted_at = Some(Utc::now());
        self.updated_at = Utc::now();
        Ok(())
    }
    
    /// Add an approval
    pub fn approve(&mut self, user_id: Uuid, user_name: String, comment: Option<String>) -> Result<(), AppError> {
        if !self.status.can_approve() {
            return Err(AppError::Validation(format!(
                "Cannot approve proposal with status {:?}",
                self.status
            )));
        }
        
        // Check if user already approved
        if self.approvals.iter().any(|a| a.user_id == user_id) {
            return Err(AppError::Validation("User has already approved this proposal".to_string()));
        }
        
        self.approvals.push(Approval {
            id: Uuid::new_v4(),
            user_id,
            user_name,
            approved_at: Utc::now(),
            comment,
        });
        self.updated_at = Utc::now();
        Ok(())
    }
    
    /// Reject the proposal
    pub fn reject(&mut self, user_id: Uuid, user_name: String, reason: String) -> Result<(), AppError> {
        if !self.status.can_approve() {
            return Err(AppError::Validation(format!(
                "Cannot reject proposal with status {:?}",
                self.status
            )));
        }
        
        self.rejections.push(Rejection {
            id: Uuid::new_v4(),
            user_id,
            user_name,
            rejected_at: Utc::now(),
            reason,
        });
        self.status = ProposalStatus::Rejected;
        self.updated_at = Utc::now();
        Ok(())
    }
    
    /// Check if proposal meets approval requirements
    pub fn check_approval_requirements(&self, policy: &ApprovalPolicy) -> ApprovalCheck {
        let approval_count = self.approvals.len() as u32;
        let meets_count = approval_count >= policy.min_approvals;
        
        // Check for auto-approval conditions
        let can_auto_approve = if let Some(ref risk) = self.risk_analysis {
            risk.safety_score >= policy.max_auto_approve_risk_score
        } else {
            false
        };
        
        ApprovalCheck {
            meets_requirements: meets_count,
            current_approvals: approval_count,
            required_approvals: policy.min_approvals,
            can_auto_approve,
            blocking_reasons: self.get_blocking_reasons(policy),
        }
    }
    
    fn get_blocking_reasons(&self, policy: &ApprovalPolicy) -> Vec<String> {
        let mut reasons = Vec::new();
        
        if self.approvals.len() < policy.min_approvals as usize {
            reasons.push(format!(
                "Need {} more approval(s)",
                policy.min_approvals - self.approvals.len() as u32
            ));
        }
        
        if policy.block_on_pii_changes && self.has_pii_changes() {
            reasons.push("PII changes require security team approval".to_string());
        }
        
        if let Some(ref risk) = self.risk_analysis {
            if risk.safety_score < 50.0 {
                reasons.push(format!(
                    "Risk score too low ({:.1}%), requires manual review",
                    risk.safety_score
                ));
            }
        }
        
        reasons
    }
    
    /// Check if proposal affects PII columns
    pub fn has_pii_changes(&self) -> bool {
        self.changes.iter().any(|c| matches!(c, SchemaChange::SetPiiClassification(_)))
    }
    
    /// Mark as approved (met all requirements)
    pub fn mark_approved(&mut self) {
        self.status = ProposalStatus::Approved;
        self.updated_at = Utc::now();
    }
    
    /// Mark as merged (executed successfully)
    pub fn mark_merged(&mut self, execution_result: ExecutionResult) {
        self.status = ProposalStatus::Merged;
        self.merged_at = Some(Utc::now());
        self.execution_result = Some(execution_result);
        self.updated_at = Utc::now();
    }
    
    /// Close without merging
    #[allow(dead_code)]
    pub fn close(&mut self) {
        self.status = ProposalStatus::Closed;
        self.updated_at = Utc::now();
    }
    
    /// Add a comment
    pub fn add_comment(&mut self, comment: Comment) {
        self.comments.push(comment);
        self.updated_at = Utc::now();
    }
}

// =============================================================================
// MIGRATION ARTIFACTS
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationArtifacts {
    /// Forward migration SQL
    pub forward_sql: String,
    
    /// Rollback SQL (to undo the migration)
    pub rollback_sql: String,
    
    /// Individual SQL statements (for step-by-step execution)
    pub statements: Vec<MigrationStatement>,
    
    /// Warnings generated during SQL generation
    pub warnings: Vec<MigrationWarning>,
    
    /// Estimated execution time
    pub estimated_duration_ms: Option<u64>,
    
    /// Generated at timestamp
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationStatement {
    pub index: usize,
    pub sql: String,
    pub rollback_sql: Option<String>,
    pub description: String,
    #[serde(default)]
    pub is_destructive: bool,
    #[serde(default)]
    pub requires_lock: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationWarning {
    pub level: WarningLevel,
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WarningLevel {
    Info,
    Warning,
    Error,
    Critical,
}

// =============================================================================
// RISK ANALYSIS CONTRACT
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAnalysis {
    /// Overall safety score (0-100, higher is safer)
    pub safety_score: f64,
    
    /// Risk level classification
    pub risk_level: RiskLevel,
    
    /// Estimated lock duration (if applicable)
    pub estimated_lock_duration_ms: Option<u64>,
    
    /// Tables affected with their risk assessment
    pub affected_tables: Vec<AffectedTableRisk>,
    
    /// Downstream impacts
    pub downstream_impacts: Vec<DownstreamImpact>,
    
    /// Individual risk factors
    pub risk_factors: Vec<RiskFactor>,
    
    /// Recommendations
    pub recommendations: Vec<Recommendation>,
    
    /// Analysis timestamp
    pub analyzed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    /// Very safe, minimal risk
    Minimal,
    /// Generally safe
    Low,
    /// Some risk, review recommended
    Medium,
    /// Significant risk, careful review required
    High,
    /// Critical risk, expert review mandatory
    Critical,
}

impl RiskLevel {
    pub fn from_score(score: f64) -> Self {
        match score {
            s if s >= 90.0 => RiskLevel::Minimal,
            s if s >= 75.0 => RiskLevel::Low,
            s if s >= 50.0 => RiskLevel::Medium,
            s if s >= 25.0 => RiskLevel::High,
            _ => RiskLevel::Critical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AffectedTableRisk {
    pub schema: String,
    pub table_name: String,
    pub size_category: SizeCategory,
    pub row_count: i64,
    pub size_bytes: i64,
    pub lock_required: bool,
    pub estimated_lock_ms: Option<u64>,
    pub is_hot_spot: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownstreamImpact {
    pub object_type: DependentObjectType,
    pub schema: String,
    pub name: String,
    pub impact_type: ImpactType,
    pub description: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ImpactType {
    /// Will break if change is applied
    Breaking,
    /// Will need update/rebuild
    RequiresUpdate,
    /// May be affected
    PotentialIssue,
    /// For informational purposes only
    Informational,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskFactor {
    pub code: String,
    pub name: String,
    pub description: String,
    pub severity: RiskSeverity,
    pub score_impact: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RiskSeverity {
    Info,
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub priority: RecommendationPriority,
    pub title: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RecommendationPriority {
    Required,
    Recommended,
    Optional,
}

// =============================================================================
// APPROVAL CHECK RESULT
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalCheck {
    pub meets_requirements: bool,
    pub current_approvals: u32,
    pub required_approvals: u32,
    pub can_auto_approve: bool,
    pub blocking_reasons: Vec<String>,
}

// =============================================================================
// PROPOSAL SERVICE
// =============================================================================

/// Service for managing proposals
pub struct ProposalService;

impl ProposalService {
    /// Generate SQL from a schema change
    pub fn generate_sql_for_change(change: &SchemaChange) -> Result<(String, Option<String>), AppError> {
        let (forward, rollback) = match change {
            SchemaChange::CreateTable(c) => {
                let columns: Vec<String> = c.columns.iter().map(|col| {
                    let mut def = format!("\"{}\" {}", col.name, col.data_type);
                    if !col.nullable {
                        def.push_str(" NOT NULL");
                    }
                    if let Some(ref default) = col.default_value {
                        def.push_str(&format!(" DEFAULT {}", default));
                    }
                    if col.is_unique && !col.is_primary_key {
                        def.push_str(" UNIQUE");
                    }
                    def
                }).collect();
                
                let mut sql = format!(
                    "CREATE TABLE \"{}\".\"{}\" (\n    {}\n",
                    c.schema, c.name, columns.join(",\n    ")
                );
                
                if let Some(ref pk_cols) = c.primary_key {
                    sql.push_str(&format!(
                        ",\n    PRIMARY KEY (\"{}\")",
                        pk_cols.join("\", \"")
                    ));
                }
                sql.push_str("\n);");
                
                let rollback = format!(
                    "DROP TABLE IF EXISTS \"{}\".\"{}\" CASCADE;",
                    c.schema, c.name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropTable(c) => {
                let cascade = if c.cascade { " CASCADE" } else { "" };
                let sql = format!(
                    "DROP TABLE \"{}\".\"{}\"{};\n-- WARNING: This will permanently delete all data!",
                    c.schema, c.table_name, cascade
                );
                (sql, None) // Cannot auto-rollback DROP TABLE
            }
            
            SchemaChange::RenameTable(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" RENAME TO \"{}\";",
                    c.schema, c.old_name, c.new_name
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" RENAME TO \"{}\";",
                    c.schema, c.new_name, c.old_name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::AddColumn(c) => {
                let mut col_def = format!("\"{}\" {}", c.column.name, c.column.data_type);
                if !c.column.nullable {
                    col_def.push_str(" NOT NULL");
                }
                if let Some(ref default) = c.column.default_value {
                    col_def.push_str(&format!(" DEFAULT {}", default));
                }
                
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" ADD COLUMN {};",
                    c.schema, c.table_name, col_def
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP COLUMN \"{}\" CASCADE;",
                    c.schema, c.table_name, c.column.name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropColumn(c) => {
                let cascade = if c.cascade { " CASCADE" } else { "" };
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP COLUMN \"{}\"{};\n-- WARNING: This will permanently delete column data!",
                    c.schema, c.table_name, c.column_name, cascade
                );
                (sql, None) // Cannot auto-rollback DROP COLUMN
            }
            
            SchemaChange::AlterColumn(c) => {
                let mut statements = Vec::new();
                
                if let Some(ref new_type) = c.new_type {
                    statements.push(format!(
                        "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" TYPE {} USING \"{}\"::{};",
                        c.schema, c.table_name, c.column_name, new_type, c.column_name, new_type
                    ));
                }
                
                if let Some(nullable) = c.set_nullable {
                    if nullable {
                        statements.push(format!(
                            "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" DROP NOT NULL;",
                            c.schema, c.table_name, c.column_name
                        ));
                    } else {
                        statements.push(format!(
                            "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" SET NOT NULL;",
                            c.schema, c.table_name, c.column_name
                        ));
                    }
                }
                
                if let Some(ref default) = c.set_default {
                    statements.push(format!(
                        "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" SET DEFAULT {};",
                        c.schema, c.table_name, c.column_name, default
                    ));
                }
                
                if c.drop_default.unwrap_or(false) {
                    statements.push(format!(
                        "ALTER TABLE \"{}\".\"{}\" ALTER COLUMN \"{}\" DROP DEFAULT;",
                        c.schema, c.table_name, c.column_name
                    ));
                }
                
                (statements.join("\n"), None)
            }
            
            SchemaChange::RenameColumn(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" RENAME COLUMN \"{}\" TO \"{}\";",
                    c.schema, c.table_name, c.old_name, c.new_name
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" RENAME COLUMN \"{}\" TO \"{}\";",
                    c.schema, c.table_name, c.new_name, c.old_name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::AddForeignKey(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" ADD CONSTRAINT \"{}\" \
                     FOREIGN KEY ({}) REFERENCES \"{}\".\"{}\" ({}) \
                     ON UPDATE {} ON DELETE {};",
                    c.source_schema, c.source_table, c.constraint_name,
                    c.source_columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", "),
                    c.referenced_schema, c.referenced_table,
                    c.referenced_columns.iter().map(|c| format!("\"{}\"", c)).collect::<Vec<_>>().join(", "),
                    c.on_update, c.on_delete
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.source_schema, c.source_table, c.constraint_name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropForeignKey(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.schema, c.table_name, c.constraint_name
                );
                (sql, None) // Need original FK definition to rollback
            }
            
            SchemaChange::AddPrimaryKey(c) => {
                let constraint_name = c.constraint_name.clone()
                    .unwrap_or_else(|| format!("{}_pkey", c.table_name));
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" ADD CONSTRAINT \"{}\" PRIMARY KEY ({});",
                    c.schema, c.table_name, constraint_name,
                    c.columns.iter().map(|col| format!("\"{}\"", col)).collect::<Vec<_>>().join(", ")
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.schema, c.table_name, constraint_name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropPrimaryKey(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.schema, c.table_name, c.constraint_name
                );
                (sql, None) // Need original PK definition to rollback
            }
            
            SchemaChange::AddUniqueConstraint(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" ADD CONSTRAINT \"{}\" UNIQUE ({});",
                    c.schema, c.table_name, c.constraint_name,
                    c.columns.iter().map(|col| format!("\"{}\"", col)).collect::<Vec<_>>().join(", ")
                );
                let rollback = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.schema, c.table_name, c.constraint_name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropUniqueConstraint(c) => {
                let sql = format!(
                    "ALTER TABLE \"{}\".\"{}\" DROP CONSTRAINT \"{}\";",
                    c.schema, c.table_name, c.constraint_name
                );
                (sql, None)
            }
            
            SchemaChange::AddIndex(c) => {
                let unique = if c.is_unique { "UNIQUE " } else { "" };
                let concurrent = if c.concurrent { "CONCURRENTLY " } else { "" };
                let sql = format!(
                    "CREATE {}INDEX {}\"{}\" ON \"{}\".\"{}\" USING {} ({});",
                    unique, concurrent, c.name, c.schema, c.table_name, c.index_type,
                    c.columns.iter().map(|col| format!("\"{}\"", col)).collect::<Vec<_>>().join(", ")
                );
                let drop_concurrent = if c.concurrent { "CONCURRENTLY " } else { "" };
                let rollback = format!(
                    "DROP INDEX {}\"{}\".\"{}\"",
                    drop_concurrent, c.schema, c.name
                );
                (sql, Some(rollback))
            }
            
            SchemaChange::DropIndex(c) => {
                let concurrent = if c.concurrent { "CONCURRENTLY " } else { "" };
                let sql = format!(
                    "DROP INDEX {}\"{}\".\"{}\"",
                    concurrent, c.schema, c.name
                );
                (sql, None)
            }
            
            // Governance changes don't generate SQL
            SchemaChange::SetPiiClassification(_) |
            SchemaChange::AddTag(_) |
            SchemaChange::RemoveTag(_) |
            SchemaChange::SetDescription(_) => {
                ("-- Governance metadata change (no SQL)".to_string(), None)
            }
        };
        
        Ok((forward, rollback))
    }
    
    /// Generate complete migration artifacts for a proposal
    pub fn generate_migration(proposal: &SchemaProposal) -> Result<MigrationArtifacts, AppError> {
        let mut forward_statements = Vec::new();
        let mut rollback_statements = Vec::new();
        let mut statements = Vec::new();
        let mut warnings = Vec::new();
        
        for (index, change) in proposal.changes.iter().enumerate() {
            if !change.modifies_database() {
                continue; // Skip governance-only changes
            }
            
            let (forward_sql, rollback_sql) = Self::generate_sql_for_change(change)?;
            
            // Check for potential issues
            match change {
                SchemaChange::DropTable(_) | SchemaChange::DropColumn(_) => {
                    warnings.push(MigrationWarning {
                        level: WarningLevel::Critical,
                        code: "DESTRUCTIVE_CHANGE".to_string(),
                        message: "This change permanently deletes data and cannot be automatically rolled back.".to_string(),
                        change_index: Some(index),
                        suggestion: Some("Ensure you have a backup before proceeding.".to_string()),
                    });
                }
                SchemaChange::AlterColumn(c) if c.new_type.is_some() => {
                    warnings.push(MigrationWarning {
                        level: WarningLevel::Warning,
                        code: "TYPE_CHANGE".to_string(),
                        message: "Type changes may fail if existing data cannot be converted.".to_string(),
                        change_index: Some(index),
                        suggestion: Some("Test on a non-production database first.".to_string()),
                    });
                }
                SchemaChange::AlterColumn(c) if c.set_nullable == Some(false) => {
                    warnings.push(MigrationWarning {
                        level: WarningLevel::Warning,
                        code: "NOT_NULL_CONSTRAINT".to_string(),
                        message: "Adding NOT NULL will fail if the column contains NULL values.".to_string(),
                        change_index: Some(index),
                        suggestion: Some("Ensure all rows have values or provide a default.".to_string()),
                    });
                }
                _ => {}
            }
            
            let requires_lock = matches!(
                change,
                SchemaChange::AddColumn(_) |
                SchemaChange::AlterColumn(_) |
                SchemaChange::DropColumn(_) |
                SchemaChange::AddForeignKey(_) |
                SchemaChange::AddPrimaryKey(_)
            );
            
            let is_destructive = matches!(
                change,
                SchemaChange::DropTable(_) |
                SchemaChange::DropColumn(_)
            );
            
            statements.push(MigrationStatement {
                index,
                sql: forward_sql.clone(),
                rollback_sql: rollback_sql.clone(),
                description: change.description(),
                is_destructive,
                requires_lock,
            });
            
            forward_statements.push(forward_sql);
            if let Some(rb) = rollback_sql {
                rollback_statements.push(rb);
            }
        }
        
        // Rollback statements need to be in reverse order
        rollback_statements.reverse();
        
        Ok(MigrationArtifacts {
            forward_sql: forward_statements.join("\n\n"),
            rollback_sql: rollback_statements.join("\n\n"),
            statements,
            warnings,
            estimated_duration_ms: None, // Will be filled by risk analysis
            generated_at: Utc::now(),
        })
    }
}
