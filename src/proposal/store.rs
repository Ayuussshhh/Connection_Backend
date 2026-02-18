//! Proposal storage
//!
//! In-memory store with PostgreSQL persistence for proposals.

use crate::error::AppError;
use crate::proposal::{Proposal, ProposalStatus, SchemaChange};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Thread-safe proposal store
pub struct ProposalStore {
    proposals: Arc<RwLock<HashMap<Uuid, Proposal>>>,
}

impl ProposalStore {
    pub fn new() -> Self {
        Self {
            proposals: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new proposal
    pub async fn create(&self, proposal: Proposal) -> Result<Proposal, AppError> {
        let mut proposals = self.proposals.write().await;
        let id = proposal.id;
        proposals.insert(id, proposal.clone());
        Ok(proposal)
    }

    /// Get a proposal by ID
    pub async fn get(&self, id: Uuid) -> Result<Proposal, AppError> {
        let proposals = self.proposals.read().await;
        proposals
            .get(&id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))
    }

    /// List all proposals (optionally filtered by connection)
    pub async fn list(&self, connection_id: Option<Uuid>) -> Vec<Proposal> {
        let proposals = self.proposals.read().await;
        proposals
            .values()
            .filter(|p| connection_id.map_or(true, |cid| p.connection_id == cid))
            .cloned()
            .collect()
    }

    /// List proposals by status
    pub async fn list_by_status(&self, status: ProposalStatus) -> Vec<Proposal> {
        let proposals = self.proposals.read().await;
        proposals
            .values()
            .filter(|p| p.status == status)
            .cloned()
            .collect()
    }

    /// Update a proposal
    pub async fn update(&self, proposal: Proposal) -> Result<Proposal, AppError> {
        let mut proposals = self.proposals.write().await;
        if !proposals.contains_key(&proposal.id) {
            return Err(AppError::NotFound(format!("Proposal {} not found", proposal.id)));
        }
        proposals.insert(proposal.id, proposal.clone());
        Ok(proposal)
    }

    /// Add a change to a proposal
    pub async fn add_change(&self, proposal_id: Uuid, change: SchemaChange) -> Result<Proposal, AppError> {
        let mut proposals = self.proposals.write().await;
        let proposal = proposals
            .get_mut(&proposal_id)
            .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", proposal_id)))?;
        
        if proposal.status != ProposalStatus::Draft {
            return Err(AppError::BadRequest(
                "Cannot modify a proposal that is not in draft status".to_string()
            ));
        }
        
        proposal.add_change(change);
        Ok(proposal.clone())
    }

    /// Update proposal status
    pub async fn update_status(&self, proposal_id: Uuid, status: ProposalStatus) -> Result<Proposal, AppError> {
        let mut proposals = self.proposals.write().await;
        let proposal = proposals
            .get_mut(&proposal_id)
            .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", proposal_id)))?;
        
        proposal.status = status;
        proposal.updated_at = chrono::Utc::now();
        Ok(proposal.clone())
    }

    /// Delete a proposal (only if draft)
    pub async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let mut proposals = self.proposals.write().await;
        let proposal = proposals
            .get(&id)
            .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
        
        if proposal.status != ProposalStatus::Draft {
            return Err(AppError::BadRequest(
                "Cannot delete a proposal that is not in draft status".to_string()
            ));
        }
        
        proposals.remove(&id);
        Ok(())
    }

    /// Get proposal count
    pub async fn count(&self) -> usize {
        let proposals = self.proposals.read().await;
        proposals.len()
    }
}

impl Default for ProposalStore {
    fn default() -> Self {
        Self::new()
    }
}
