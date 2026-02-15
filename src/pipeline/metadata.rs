//! Metadata Storage Layer
//!
//! Stores proposals, schema snapshots, governance data, and audit logs.
//! Uses in-memory storage with optional persistence (can be extended to SQLite/Postgres).

use crate::error::AppError;
use crate::introspection::SchemaSnapshot;
use crate::pipeline::mirror::SemanticMap;
use crate::pipeline::proposal::SchemaProposal;
use crate::pipeline::types::*;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{debug, info};
use uuid::Uuid;

// =============================================================================
// METADATA STORE
// =============================================================================

/// In-memory metadata store (can be extended to persistent storage)
pub struct MetadataStore {
    /// Schema snapshots by connection ID -> version
    snapshots: RwLock<HashMap<Uuid, Vec<SchemaSnapshot>>>,
    
    /// Semantic maps by connection ID
    semantic_maps: RwLock<HashMap<Uuid, SemanticMap>>,
    
    /// Proposals by ID
    proposals: RwLock<HashMap<Uuid, SchemaProposal>>,
    
    /// Proposals index by connection ID
    proposals_by_connection: RwLock<HashMap<Uuid, Vec<Uuid>>>,
    
    /// Governance tags by target path
    #[allow(dead_code)]
    governance_tags: RwLock<HashMap<String, GovernanceMetadata>>,
    
    /// Audit log
    audit_log: RwLock<Vec<AuditEntry>>,
}

impl MetadataStore {
    pub fn new() -> Self {
        Self {
            snapshots: RwLock::new(HashMap::new()),
            semantic_maps: RwLock::new(HashMap::new()),
            proposals: RwLock::new(HashMap::new()),
            proposals_by_connection: RwLock::new(HashMap::new()),
            governance_tags: RwLock::new(HashMap::new()),
            audit_log: RwLock::new(Vec::new()),
        }
    }
    
    // =========================================================================
    // SCHEMA SNAPSHOTS
    // =========================================================================
    
    /// Save a new schema snapshot
    pub async fn save_snapshot(&self, snapshot: SchemaSnapshot) -> Result<(), AppError> {
        let mut snapshots = self.snapshots.write().await;
        let entry = snapshots.entry(snapshot.connection_id).or_insert_with(Vec::new);
        
        // Set version number
        let version = entry.len() as u64 + 1;
        let mut snapshot = snapshot;
        snapshot.version = version;
        
        entry.push(snapshot.clone());
        
        self.log_audit(AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: None,
            action: AuditAction::SchemaSnapshot,
            resource_type: "schema_snapshot".to_string(),
            resource_id: Some(snapshot.id),
            details: Some(serde_json::json!({
                "connection_id": snapshot.connection_id,
                "version": version,
                "checksum": snapshot.checksum
            })),
            ip_address: None,
        }).await;
        
        debug!("Saved schema snapshot v{} for connection {}", version, snapshot.connection_id);
        Ok(())
    }
    
    /// Get latest snapshot for a connection
    pub async fn get_latest_snapshot(&self, connection_id: Uuid) -> Option<SchemaSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots.get(&connection_id)
            .and_then(|list| list.last().cloned())
    }
    
    /// Get snapshot by version
    #[allow(dead_code)]
    pub async fn get_snapshot(&self, connection_id: Uuid, version: u64) -> Option<SchemaSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots.get(&connection_id)
            .and_then(|list| list.iter().find(|s| s.version == version).cloned())
    }
    
    /// Get all snapshot versions for a connection
    #[allow(dead_code)]
    pub async fn list_snapshot_versions(&self, connection_id: Uuid) -> Vec<SnapshotSummary> {
        let snapshots = self.snapshots.read().await;
        snapshots.get(&connection_id)
            .map(|list| {
                list.iter().map(|s| SnapshotSummary {
                    id: s.id,
                    version: s.version,
                    captured_at: s.captured_at,
                    checksum: s.checksum.clone(),
                    table_count: s.tables.len(),
                }).collect()
            })
            .unwrap_or_default()
    }
    
    // =========================================================================
    // SEMANTIC MAPS
    // =========================================================================
    
    /// Save a semantic map
    pub async fn save_semantic_map(&self, map: SemanticMap) {
        let mut maps = self.semantic_maps.write().await;
        maps.insert(map.connection_id, map);
    }
    
    /// Get semantic map for a connection
    pub async fn get_semantic_map(&self, connection_id: Uuid) -> Option<SemanticMap> {
        let maps = self.semantic_maps.read().await;
        maps.get(&connection_id).cloned()
    }
    
    // =========================================================================
    // PROPOSALS
    // =========================================================================
    
    /// Create a new proposal
    pub async fn create_proposal(&self, proposal: SchemaProposal) -> Result<SchemaProposal, AppError> {
        let proposal_id = proposal.id;
        let connection_id = proposal.connection_id;
        
        {
            let mut proposals = self.proposals.write().await;
            proposals.insert(proposal_id, proposal.clone());
        }
        
        {
            let mut by_conn = self.proposals_by_connection.write().await;
            by_conn.entry(connection_id).or_default().push(proposal_id);
        }
        
        self.log_audit(AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: Some(proposal.author_id),
            action: AuditAction::ProposalCreated,
            resource_type: "proposal".to_string(),
            resource_id: Some(proposal_id),
            details: Some(serde_json::json!({
                "title": proposal.title,
                "connection_id": connection_id,
                "change_count": proposal.changes.len()
            })),
            ip_address: None,
        }).await;
        
        info!("Created proposal '{}' (id: {})", proposal.title, proposal_id);
        Ok(proposal)
    }
    
    /// Update an existing proposal
    pub async fn update_proposal(&self, proposal: SchemaProposal) -> Result<(), AppError> {
        let mut proposals = self.proposals.write().await;
        
        if !proposals.contains_key(&proposal.id) {
            return Err(AppError::NotFound(format!("Proposal {} not found", proposal.id)));
        }
        
        proposals.insert(proposal.id, proposal);
        Ok(())
    }
    
    /// Get a proposal by ID
    pub async fn get_proposal(&self, id: Uuid) -> Option<SchemaProposal> {
        let proposals = self.proposals.read().await;
        proposals.get(&id).cloned()
    }
    
    /// List proposals for a connection
    pub async fn list_proposals(&self, connection_id: Uuid, status_filter: Option<ProposalStatus>) -> Vec<ProposalSummary> {
        let proposals = self.proposals.read().await;
        let by_conn = self.proposals_by_connection.read().await;
        
        by_conn.get(&connection_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| proposals.get(id))
                    .filter(|p| status_filter.map(|s| p.status == s).unwrap_or(true))
                    .map(|p| ProposalSummary {
                        id: p.id,
                        title: p.title.clone(),
                        status: p.status,
                        author_name: p.author_name.clone(),
                        change_count: p.changes.len(),
                        approval_count: p.approvals.len(),
                        safety_score: p.risk_analysis.as_ref().map(|r| r.safety_score),
                        created_at: p.created_at,
                        updated_at: p.updated_at,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    
    /// List all proposals (across all connections)
    pub async fn list_all_proposals(&self, status_filter: Option<ProposalStatus>) -> Vec<ProposalSummary> {
        let proposals = self.proposals.read().await;
        
        proposals.values()
            .filter(|p| status_filter.map(|s| p.status == s).unwrap_or(true))
            .map(|p| ProposalSummary {
                id: p.id,
                title: p.title.clone(),
                status: p.status,
                author_name: p.author_name.clone(),
                change_count: p.changes.len(),
                approval_count: p.approvals.len(),
                safety_score: p.risk_analysis.as_ref().map(|r| r.safety_score),
                created_at: p.created_at,
                updated_at: p.updated_at,
            })
            .collect()
    }
    
    // =========================================================================
    // GOVERNANCE
    // =========================================================================
    
    /// Set governance metadata for a target
    #[allow(dead_code)]
    pub async fn set_governance(&self, target_path: String, metadata: GovernanceMetadata) {
        let mut tags = self.governance_tags.write().await;
        tags.insert(target_path, metadata);
    }
    
    /// Get governance metadata for a target
    #[allow(dead_code)]
    pub async fn get_governance(&self, target_path: &str) -> Option<GovernanceMetadata> {
        let tags = self.governance_tags.read().await;
        tags.get(target_path).cloned()
    }
    
    /// List all governance metadata for a connection
    #[allow(dead_code)]
    pub async fn list_governance(&self, _connection_id: Uuid) -> Vec<(String, GovernanceMetadata)> {
        let tags = self.governance_tags.read().await;
        // Filter by connection (paths start with connection tables)
        tags.iter()
            .filter(|(_path, _)| {
                // TODO: Better filtering by connection
                true
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
    
    // =========================================================================
    // AUDIT LOG
    // =========================================================================
    
    /// Log an audit entry
    pub async fn log_audit(&self, entry: AuditEntry) {
        let mut log = self.audit_log.write().await;
        log.push(entry);
    }
    
    /// Get audit log entries
    pub async fn get_audit_log(
        &self,
        resource_type: Option<&str>,
        resource_id: Option<Uuid>,
        limit: usize,
    ) -> Vec<AuditEntry> {
        let log = self.audit_log.read().await;
        
        log.iter()
            .rev() // Most recent first
            .filter(|e| {
                resource_type.map(|t| e.resource_type == t).unwrap_or(true)
                    && resource_id.map(|id| e.resource_id == Some(id)).unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect()
    }
}

impl Default for MetadataStore {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// DATA TYPES
// =============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct SnapshotSummary {
    pub id: Uuid,
    pub version: u64,
    pub captured_at: DateTime<Utc>,
    pub checksum: String,
    pub table_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalSummary {
    pub id: Uuid,
    pub title: String,
    pub status: ProposalStatus,
    pub author_name: String,
    pub change_count: usize,
    pub approval_count: usize,
    pub safety_score: Option<f64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceMetadata {
    pub pii_level: Option<PiiLevel>,
    pub tags: Vec<String>,
    pub description: Option<String>,
    pub owner: Option<String>,
    pub retention_days: Option<i32>,
    pub compliance_notes: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: Option<Uuid>,
}

impl Default for GovernanceMetadata {
    fn default() -> Self {
        Self {
            pii_level: None,
            tags: Vec::new(),
            description: None,
            owner: None,
            retention_days: None,
            compliance_notes: None,
            updated_at: Utc::now(),
            updated_by: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub user_id: Option<Uuid>,
    pub action: AuditAction,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub details: Option<serde_json::Value>,
    pub ip_address: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    // Connection actions
    ConnectionCreated,
    ConnectionDeleted,
    ConnectionIntrospect,
    
    // Schema actions
    SchemaSnapshot,
    SchemaDriftDetected,
    
    // Proposal actions
    ProposalCreated,
    ProposalUpdated,
    ProposalSubmitted,
    ProposalApproved,
    ProposalRejected,
    ProposalExecuted,
    ProposalRolledBack,
    ProposalClosed,
    
    // Governance actions
    GovernanceUpdated,
    PiiClassificationChanged,
    
    // Comment actions
    CommentAdded,
    CommentResolved,
}
