//! Core types for the Governance Pipeline
//!
//! Contains all shared types, enums, and data structures used across
//! the pipeline stages.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// SCHEMA CHANGE TYPES
// =============================================================================

/// Individual schema change within a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SchemaChange {
    // Table operations
    CreateTable(CreateTableChange),
    DropTable(DropTableChange),
    RenameTable(RenameTableChange),
    
    // Column operations
    AddColumn(AddColumnChange),
    DropColumn(DropColumnChange),
    AlterColumn(AlterColumnChange),
    RenameColumn(RenameColumnChange),
    
    // Constraint operations
    AddForeignKey(AddForeignKeyChange),
    DropForeignKey(DropForeignKeyChange),
    AddPrimaryKey(AddPrimaryKeyChange),
    DropPrimaryKey(DropPrimaryKeyChange),
    AddUniqueConstraint(AddUniqueConstraintChange),
    DropUniqueConstraint(DropUniqueConstraintChange),
    
    // Index operations
    AddIndex(AddIndexChange),
    DropIndex(DropIndexChange),
    
    // Governance changes (metadata only, no SQL)
    SetPiiClassification(SetPiiChange),
    AddTag(AddTagChange),
    RemoveTag(RemoveTagChange),
    SetDescription(SetDescriptionChange),
}

impl SchemaChange {
    /// Returns true if this change modifies the actual database schema
    pub fn modifies_database(&self) -> bool {
        !matches!(
            self,
            SchemaChange::SetPiiClassification(_)
                | SchemaChange::AddTag(_)
                | SchemaChange::RemoveTag(_)
                | SchemaChange::SetDescription(_)
        )
    }
    
    /// Get a human-readable description of the change
    pub fn description(&self) -> String {
        match self {
            Self::CreateTable(c) => format!("Create table '{}.{}'", c.schema, c.name),
            Self::DropTable(c) => format!("Drop table '{}.{}'", c.schema, c.table_name),
            Self::RenameTable(c) => format!("Rename table '{}' to '{}'", c.old_name, c.new_name),
            Self::AddColumn(c) => format!("Add column '{}' to '{}.{}'", c.column.name, c.schema, c.table_name),
            Self::DropColumn(c) => format!("Drop column '{}' from '{}.{}'", c.column_name, c.schema, c.table_name),
            Self::AlterColumn(c) => format!("Alter column '{}' in '{}.{}'", c.column_name, c.schema, c.table_name),
            Self::RenameColumn(c) => format!("Rename column '{}' to '{}' in '{}'", c.old_name, c.new_name, c.table_name),
            Self::AddForeignKey(c) => format!("Add foreign key '{}' from '{}.{}' to '{}.{}'", 
                c.constraint_name, c.source_schema, c.source_table, c.referenced_schema, c.referenced_table),
            Self::DropForeignKey(c) => format!("Drop foreign key '{}'", c.constraint_name),
            Self::AddPrimaryKey(c) => format!("Add primary key to '{}.{}'", c.schema, c.table_name),
            Self::DropPrimaryKey(c) => format!("Drop primary key from '{}.{}'", c.schema, c.table_name),
            Self::AddUniqueConstraint(c) => format!("Add unique constraint '{}' to '{}.{}'", c.constraint_name, c.schema, c.table_name),
            Self::DropUniqueConstraint(c) => format!("Drop unique constraint '{}'", c.constraint_name),
            Self::AddIndex(c) => format!("Add index '{}' on '{}.{}'", c.name, c.schema, c.table_name),
            Self::DropIndex(c) => format!("Drop index '{}'", c.name),
            Self::SetPiiClassification(c) => format!("Set PII classification for '{}.{}'", c.table_name, c.column_name.as_deref().unwrap_or("*")),
            Self::AddTag(c) => format!("Add tag '{}' to '{}'", c.tag, c.target_path),
            Self::RemoveTag(c) => format!("Remove tag '{}' from '{}'", c.tag, c.target_path),
            Self::SetDescription(c) => format!("Set description for '{}'", c.target_path),
        }
    }
}

// =============================================================================
// CHANGE DETAIL TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTableChange {
    pub schema: String,
    pub name: String,
    pub columns: Vec<ColumnDefinition>,
    pub primary_key: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropTableChange {
    pub schema: String,
    pub table_name: String,
    #[serde(default)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_column: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropColumnChange {
    pub schema: String,
    pub table_name: String,
    pub column_name: String,
    #[serde(default)]
    pub cascade: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlterColumnChange {
    pub schema: String,
    pub table_name: String,
    pub column_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_nullable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set_default: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drop_default: Option<bool>,
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
    pub constraint_name: String,
    pub source_schema: String,
    pub source_table: String,
    pub source_columns: Vec<String>,
    pub referenced_schema: String,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    #[serde(default = "default_action")]
    pub on_update: String,
    #[serde(default = "default_action")]
    pub on_delete: String,
}

fn default_action() -> String {
    "NO ACTION".to_string()
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
pub struct AddPrimaryKeyChange {
    pub schema: String,
    pub table_name: String,
    pub columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraint_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropPrimaryKeyChange {
    pub schema: String,
    pub table_name: String,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddUniqueConstraintChange {
    pub schema: String,
    pub table_name: String,
    pub columns: Vec<String>,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropUniqueConstraintChange {
    pub schema: String,
    pub table_name: String,
    pub constraint_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddIndexChange {
    pub schema: String,
    pub table_name: String,
    pub name: String,
    pub columns: Vec<String>,
    #[serde(default)]
    pub is_unique: bool,
    #[serde(default = "default_index_type")]
    pub index_type: String,
    #[serde(default)]
    pub concurrent: bool,
}

fn default_index_type() -> String {
    "btree".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropIndexChange {
    pub schema: String,
    pub name: String,
    #[serde(default)]
    pub concurrent: bool,
}

// Governance changes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetPiiChange {
    pub schema: String,
    pub table_name: String,
    pub column_name: Option<String>, // None means table-level
    pub level: PiiLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddTagChange {
    pub target_path: String, // "schema.table" or "schema.table.column"
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveTagChange {
    pub target_path: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetDescriptionChange {
    pub target_path: String,
    pub description: String,
}

// =============================================================================
// COLUMN DEFINITION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDefinition {
    pub name: String,
    pub data_type: String,
    #[serde(default = "default_nullable")]
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default)]
    pub is_primary_key: bool,
    #[serde(default)]
    pub is_unique: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_constraint: Option<String>,
}

fn default_nullable() -> bool {
    true
}

// =============================================================================
// PII CLASSIFICATION
// =============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PiiLevel {
    /// No PII - safe for any use
    None,
    /// Internal use only
    Internal,
    /// Personally Identifiable Information (name, email, phone)
    Confidential,
    /// Sensitive PII (SSN, financial data, health records)
    Restricted,
    /// Highest sensitivity (authentication credentials, secrets)
    Secret,
}

#[allow(dead_code)]
impl PiiLevel {
    pub fn requires_approval(&self) -> bool {
        matches!(self, PiiLevel::Confidential | PiiLevel::Restricted | PiiLevel::Secret)
    }
    
    pub fn risk_multiplier(&self) -> f64 {
        match self {
            PiiLevel::None => 1.0,
            PiiLevel::Internal => 1.2,
            PiiLevel::Confidential => 1.5,
            PiiLevel::Restricted => 2.0,
            PiiLevel::Secret => 3.0,
        }
    }
}

// =============================================================================
// PROPOSAL STATUS
// =============================================================================

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProposalStatus {
    /// Still editing, not visible to others
    Draft,
    /// Ready for review
    Open,
    /// Has all required approvals
    Approved,
    /// Change request or rejected
    Rejected,
    /// Successfully executed
    Merged,
    /// Closed without executing
    Closed,
}

impl ProposalStatus {
    pub fn can_edit(&self) -> bool {
        matches!(self, ProposalStatus::Draft)
    }
    
    pub fn can_submit_for_review(&self) -> bool {
        matches!(self, ProposalStatus::Draft)
    }
    
    pub fn can_approve(&self) -> bool {
        matches!(self, ProposalStatus::Open)
    }
    
    pub fn can_execute(&self) -> bool {
        matches!(self, ProposalStatus::Approved)
    }
}

// =============================================================================
// APPROVAL & REVIEW
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Approval {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub approved_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejection {
    pub id: Uuid,
    pub user_id: Uuid,
    pub user_name: String,
    pub rejected_at: DateTime<Utc>,
    pub reason: String,
}

// =============================================================================
// COMMENTS
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_name: String,
    pub target: CommentTarget,
    pub content: String,
    #[serde(default)]
    pub resolved: bool,
    #[serde(default)]
    pub replies: Vec<Comment>,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CommentTarget {
    /// Comment on the entire proposal
    Proposal { proposal_id: Uuid },
    /// Comment on a specific table
    Table { schema: String, table_name: String },
    /// Comment on a specific column
    Column { schema: String, table_name: String, column_name: String },
    /// Comment on a specific change within the proposal
    Change { change_index: usize },
    /// General comment on the connection
    Connection { connection_id: Uuid },
}

// =============================================================================
// EXECUTION RESULT
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResult {
    pub success: bool,
    pub executed_at: DateTime<Utc>,
    pub executed_by: Uuid,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub sql_executed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_sql: Option<String>,
    #[serde(default)]
    pub was_rolled_back: bool,
}

// =============================================================================
// USER & COLLABORATION
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct UserPresence {
    pub user_id: Uuid,
    pub user_name: String,
    pub connection_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor_position: Option<Position>,
    #[serde(default)]
    pub selected_elements: Vec<String>,
    pub last_active: DateTime<Utc>,
    pub color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[allow(dead_code)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

// =============================================================================
// APPROVAL POLICY
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalPolicy {
    /// Minimum number of approvals required
    #[serde(default = "default_min_approvals")]
    pub min_approvals: u32,
    /// Roles that can approve
    #[serde(default)]
    pub required_roles: Vec<String>,
    /// Environments that auto-approve
    #[serde(default)]
    pub auto_approve_environments: Vec<String>,
    /// Require CI/CD pass before merge
    #[serde(default)]
    pub require_ci_pass: bool,
    /// Block changes to PII columns without security approval
    #[serde(default = "default_true")]
    pub block_on_pii_changes: bool,
    /// Maximum risk score allowed for auto-approval
    #[serde(default = "default_max_risk")]
    pub max_auto_approve_risk_score: f64,
}

fn default_min_approvals() -> u32 {
    1
}

fn default_true() -> bool {
    true
}

fn default_max_risk() -> f64 {
    80.0
}

impl Default for ApprovalPolicy {
    fn default() -> Self {
        Self {
            min_approvals: 1,
            required_roles: vec![],
            auto_approve_environments: vec!["development".to_string()],
            require_ci_pass: false,
            block_on_pii_changes: true,
            max_auto_approve_risk_score: 80.0,
        }
    }
}

// =============================================================================
// TABLE STATISTICS (for risk analysis)
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableStatistics {
    pub schema: String,
    pub table_name: String,
    pub row_count: i64,
    pub table_size_bytes: i64,
    pub index_size_bytes: i64,
    pub total_size_bytes: i64,
    pub last_vacuum: Option<DateTime<Utc>>,
    pub last_analyze: Option<DateTime<Utc>>,
    #[serde(default)]
    pub is_partitioned: bool,
    #[serde(default)]
    pub has_triggers: bool,
    pub dead_tuples: i64,
}

impl TableStatistics {
    pub fn size_category(&self) -> SizeCategory {
        let size_mb = self.total_size_bytes / (1024 * 1024);
        match size_mb {
            0..=100 => SizeCategory::Small,
            101..=1024 => SizeCategory::Medium,
            1025..=10240 => SizeCategory::Large,
            _ => SizeCategory::VeryLarge,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SizeCategory {
    Small,
    Medium,
    Large,
    VeryLarge,
}

// =============================================================================
// DEPENDENT OBJECTS
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DependentObject {
    pub object_type: DependentObjectType,
    pub schema: String,
    pub name: String,
    pub dependency_type: DependencyType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DependentObjectType {
    Table,
    View,
    MaterializedView,
    Function,
    Procedure,
    Trigger,
    Index,
    ForeignKey,
    Policy,
    Sequence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    /// Hard dependency - will break if removed
    Hard,
    /// Soft dependency - may need adjustment
    Soft,
    /// Cascading - will be automatically affected
    Cascade,
}
