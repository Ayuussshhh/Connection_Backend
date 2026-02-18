//! Metadata storage for the governance pipeline
//!
//! Stores proposals, audit logs, and schema snapshots.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Metadata store for governance data
pub struct MetadataStore {
    proposals: Arc<RwLock<HashMap<Uuid, ProposalSummary>>>,
    audit_log: Arc<RwLock<Vec<AuditEntry>>>,
}

impl MetadataStore {
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(HashMap::new())),
            audit_log: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn add_proposal(&self, proposal: ProposalSummary) {
        let mut proposals = self.proposals.write().await;
        proposals.insert(proposal.id, proposal);
    }

    pub async fn get_proposal(&self, id: Uuid) -> Option<ProposalSummary> {
        let proposals = self.proposals.read().await;
        proposals.get(&id).cloned()
    }

    pub async fn list_proposals(&self) -> Vec<ProposalSummary> {
        let proposals = self.proposals.read().await;
        proposals.values().cloned().collect()
    }

    pub async fn add_audit_entry(&self, entry: AuditEntry) {
        let mut log = self.audit_log.write().await;
        log.push(entry);
    }

    pub async fn get_audit_log(&self) -> Vec<AuditEntry> {
        let log = self.audit_log.read().await;
        log.clone()
    }
}

impl Default for MetadataStore {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary of a proposal for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalSummary {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub title: String,
    pub description: String,
    pub status: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub change_count: usize,
}

/// Audit log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: Uuid,
    pub action: AuditAction,
    pub actor: String,
    pub target_type: String,
    pub target_id: String,
    pub details: Option<String>,
    pub timestamp: DateTime<Utc>,
}

impl AuditEntry {
    pub fn new(action: AuditAction, actor: &str, target_type: &str, target_id: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            action,
            actor: actor.to_string(),
            target_type: target_type.to_string(),
            target_id: target_id.to_string(),
            details: None,
            timestamp: Utc::now(),
        }
    }

    pub fn with_details(mut self, details: &str) -> Self {
        self.details = Some(details.to_string());
        self
    }
}

/// Audit action types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    ProposalCreated,
    ProposalUpdated,
    ProposalSubmitted,
    ProposalApproved,
    ProposalRejected,
    ProposalExecuted,
    ProposalRolledBack,
    SchemaChanged,
    ConnectionCreated,
    ConnectionDeleted,
}
