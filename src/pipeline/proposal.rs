//! Proposal service - Schema change proposal management (legacy)

use crate::error::AppError;
use crate::pipeline::types::SchemaChange;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Proposal service for managing schema change proposals
pub struct ProposalService {
    proposals: Arc<RwLock<HashMap<Uuid, SchemaProposal>>>,
}

impl ProposalService {
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn create(&self, proposal: SchemaProposal) -> Result<SchemaProposal, AppError> {
        let mut proposals = self.proposals.write().await;
        proposals.insert(proposal.id, proposal.clone());
        Ok(proposal)
    }

    pub async fn get(&self, id: Uuid) -> Option<SchemaProposal> {
        let proposals = self.proposals.read().await;
        proposals.get(&id).cloned()
    }

    pub async fn update(&self, proposal: SchemaProposal) -> Result<SchemaProposal, AppError> {
        let mut proposals = self.proposals.write().await;
        proposals.insert(proposal.id, proposal.clone());
        Ok(proposal)
    }

    pub async fn list(&self) -> Vec<SchemaProposal> {
        let proposals = self.proposals.read().await;
        proposals.values().cloned().collect()
    }
}

impl Default for ProposalService {
    fn default() -> Self {
        Self::new()
    }
}

/// A schema change proposal (like a GitHub PR for databases)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaProposal {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: ProposalStatus,
    pub changes: Vec<SchemaChange>,
    pub comments: Vec<Comment>,
    pub migration: Option<MigrationArtifacts>,
    pub risk_analysis: Option<RiskAnalysis>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    pub approved_at: Option<DateTime<Utc>>,
    pub approved_by: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
}

impl SchemaProposal {
    pub fn new(connection_id: Uuid, title: String, description: String, created_by: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            connection_id,
            title,
            description,
            status: ProposalStatus::Draft,
            changes: Vec::new(),
            comments: Vec::new(),
            migration: None,
            risk_analysis: None,
            created_by,
            created_at: now,
            updated_at: now,
            submitted_at: None,
            approved_at: None,
            approved_by: None,
            executed_at: None,
        }
    }
}

/// Proposal status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Draft,
    PendingReview,
    Approved,
    Rejected,
    Executing,
    Executed,
    Failed,
    RolledBack,
}

/// A comment on a proposal
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: Uuid,
    pub author: String,
    pub content: String,
    pub target: CommentTarget,
    pub created_at: DateTime<Utc>,
}

/// Target of a comment
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommentTarget {
    Proposal,
    Change { index: usize },
}

/// Migration artifacts
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationArtifacts {
    pub up_sql: String,
    pub down_sql: String,
    pub generated_at: DateTime<Utc>,
}

/// Risk analysis results
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAnalysis {
    pub overall_risk: RiskLevel,
    pub score: u32,
    pub warnings: Vec<String>,
    pub recommendations: Vec<String>,
    pub estimated_duration_secs: u64,
    pub requires_downtime: bool,
    pub affected_tables: Vec<String>,
    pub analyzed_at: DateTime<Utc>,
}

/// Risk level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}
