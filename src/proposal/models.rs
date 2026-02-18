//! Proposal data models
//!
//! Defines the structure for schema change proposals.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Proposal status in the governance workflow
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    /// Draft - being edited
    Draft,
    /// Submitted for review
    PendingReview,
    /// Approved and ready for execution
    Approved,
    /// Rejected by reviewer
    Rejected,
    /// Currently executing
    Executing,
    /// Successfully executed
    Executed,
    /// Execution failed
    Failed,
    /// Rolled back
    RolledBack,
}

impl Default for ProposalStatus {
    fn default() -> Self {
        ProposalStatus::Draft
    }
}

/// A schema change proposal (like a GitHub PR for databases)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub id: Uuid,
    /// Connection this proposal applies to
    pub connection_id: Uuid,
    /// User who created the proposal
    pub author_id: Uuid,
    /// Human-readable title
    pub title: String,
    /// Detailed description of changes
    pub description: Option<String>,
    /// Current status
    pub status: ProposalStatus,
    /// All changes in this proposal
    pub changes: Vec<SchemaChange>,
    /// Generated migration SQL (available after simulation)
    pub migration_sql: Option<String>,
    /// Rollback SQL (available after simulation)
    pub rollback_sql: Option<String>,
    /// Risk analysis results
    pub risk_analysis: Option<RiskAnalysis>,
    /// Comments and discussion
    pub comments: Vec<Comment>,
    /// Approval/rejection records
    pub reviews: Vec<Review>,
    /// When the proposal was created
    pub created_at: DateTime<Utc>,
    /// Last update time
    pub updated_at: DateTime<Utc>,
    /// When it was executed (if applicable)
    pub executed_at: Option<DateTime<Utc>>,
}

impl Proposal {
    pub fn new(connection_id: Uuid, author_id: Uuid, title: String, description: Option<String>) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            connection_id,
            author_id,
            title,
            description,
            status: ProposalStatus::Draft,
            changes: Vec::new(),
            migration_sql: None,
            rollback_sql: None,
            risk_analysis: None,
            comments: Vec::new(),
            reviews: Vec::new(),
            created_at: now,
            updated_at: now,
            executed_at: None,
        }
    }

    pub fn add_change(&mut self, change: SchemaChange) {
        self.changes.push(change);
        self.updated_at = Utc::now();
        // Invalidate generated SQL when changes are made
        self.migration_sql = None;
        self.rollback_sql = None;
        self.risk_analysis = None;
    }
}

/// Types of schema changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SchemaChange {
    /// Create a new table
    CreateTable(CreateTableChange),
    /// Drop an existing table
    DropTable(DropTableChange),
    /// Rename a table
    RenameTable(RenameTableChange),
    /// Add a column to a table
    AddColumn(AddColumnChange),
    /// Drop a column
    DropColumn(DropColumnChange),
    /// Modify a column
    ModifyColumn(ModifyColumnChange),
    /// Rename a column
    RenameColumn(RenameColumnChange),
    /// Add a foreign key
    AddForeignKey(AddForeignKeyChange),
    /// Drop a foreign key
    DropForeignKey(DropForeignKeyChange),
    /// Add an index
    AddIndex(AddIndexChange),
    /// Drop an index
    DropIndex(DropIndexChange),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableChange {
    pub schema: String,
    pub table_name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropTableChange {
    pub schema: String,
    pub table_name: String,
    pub cascade: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameTableChange {
    pub schema: String,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddColumnChange {
    pub schema: String,
    pub table_name: String,
    pub column: ColumnDefinition,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropColumnChange {
    pub schema: String,
    pub table_name: String,
    pub column_name: String,
    pub cascade: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModifyColumnChange {
    pub schema: String,
    pub table_name: String,
    pub column_name: String,
    pub new_type: Option<String>,
    pub new_nullable: Option<bool>,
    pub new_default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameColumnChange {
    pub schema: String,
    pub table_name: String,
    pub old_name: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddForeignKeyChange {
    pub constraint_name: Option<String>,
    pub source_schema: String,
    pub source_table: String,
    pub source_columns: Vec<String>,
    pub target_schema: String,
    pub target_table: String,
    pub target_columns: Vec<String>,
    pub on_delete: Option<String>,
    pub on_update: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropForeignKeyChange {
    pub schema: String,
    pub table_name: String,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddIndexChange {
    pub index_name: Option<String>,
    pub schema: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub unique: bool,
    pub concurrent: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropIndexChange {
    pub schema: String,
    pub index_name: String,
    pub concurrent: bool,
}

/// Column definition for new tables/columns
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub is_primary_key: bool,
    /// User-friendly label (for non-technical users)
    pub label: Option<String>,
    /// Description for documentation
    pub description: Option<String>,
    /// Is this a PII field? (for compliance)
    pub is_pii: bool,
}

/// Risk analysis results from simulation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAnalysis {
    /// Overall risk score (0-100)
    pub risk_score: u8,
    /// Risk level classification
    pub risk_level: RiskLevel,
    /// Estimated execution time in seconds
    pub estimated_duration_seconds: f64,
    /// Tables that will be locked
    pub locked_tables: Vec<String>,
    /// Potential downtime in seconds
    pub potential_downtime_seconds: f64,
    /// Downstream impacts (dependent tables/views)
    pub downstream_impacts: Vec<DownstreamImpact>,
    /// Individual risk factors
    pub risk_factors: Vec<RiskFactor>,
    /// Recommendations
    pub recommendations: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownstreamImpact {
    pub object_type: String,
    pub object_name: String,
    pub impact_type: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskFactor {
    pub category: String,
    pub description: String,
    pub severity: RiskLevel,
    pub mitigation: Option<String>,
}

/// Comment on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

/// Review decision
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub id: Uuid,
    pub reviewer_id: Uuid,
    pub reviewer_name: String,
    pub decision: ReviewDecision,
    pub comment: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReviewDecision {
    Approved,
    Rejected,
    RequestChanges,
}
