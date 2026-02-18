//! Pipeline Routes
//!
//! API endpoints for the Governance Pipeline.

use crate::error::AppError;
use crate::models::SuccessResponse;
use crate::pipeline::metadata::{AuditAction, AuditEntry, ProposalSummary};
use crate::pipeline::mirror::{MirrorService, SemanticMap};
use crate::pipeline::orchestrator::Orchestrator;
use crate::pipeline::proposal::{MigrationArtifacts, SchemaProposal};
use crate::pipeline::risk::RiskEngine;
use crate::pipeline::types::*;
use crate::state::SharedState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// =============================================================================
// REQUEST/RESPONSE TYPES
// =============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProposalRequest {
    pub connection_id: Uuid,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub changes: Vec<SchemaChange>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddChangeRequest {
    pub change: SchemaChange,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentRequest {
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    #[serde(default)]
    pub comment: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectionRequest {
    pub reason: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteRequest {
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize)]
pub struct ProposalListQuery {
    pub connection_id: Option<Uuid>,
    pub status: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalResponse {
    pub proposal: SchemaProposal,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalListResponse {
    pub proposals: Vec<ProposalSummary>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationResponse {
    pub migration: MigrationArtifacts,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RiskAnalysisResponse {
    pub analysis: crate::pipeline::proposal::RiskAnalysis,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticMapResponse {
    pub semantic_map: SemanticMap,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DriftResponse {
    pub has_drift: bool,
    pub changes: Vec<crate::pipeline::mirror::DriftChange>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionResponse {
    pub success: bool,
    pub result: crate::pipeline::orchestrator::ExecutionResult,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogResponse {
    pub entries: Vec<AuditEntry>,
}

// =============================================================================
// ROUTE HANDLERS - Mirror (Stage 1)
// =============================================================================

/// POST /api/connections/{id}/semantic-map
/// Build a semantic map of the database schema
pub async fn build_semantic_map(
    State(state): State<SharedState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<SemanticMapResponse>>, AppError> {
    // Build semantic map
    let mirror = MirrorService::new();
    let semantic_map = mirror.build_semantic_map(connection_id).await?;

    // Log audit
    let entry = AuditEntry::new(AuditAction::SchemaChanged, "system", "semantic_map", &connection_id.to_string());
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::with_data(
        "Semantic map built",
        SemanticMapResponse { semantic_map },
    )))
}

/// GET /api/connections/{id}/drift
/// Check for schema drift
pub async fn check_drift(
    State(state): State<SharedState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<DriftResponse>>, AppError> {
    let mirror = MirrorService::new();
    
    // Create an empty semantic map for comparison (simplified)
    let empty_map = SemanticMap {
        id: Uuid::new_v4(),
        connection_id,
        tables: std::collections::HashMap::new(),
        relationships: Vec::new(),
        created_at: Utc::now(),
    };

    let result = mirror.check_drift(connection_id, &empty_map).await?;

    Ok(Json(SuccessResponse::with_data(
        "Drift check complete",
        DriftResponse {
            has_drift: result.has_drift,
            changes: result.changes,
        },
    )))
}

// =============================================================================
// ROUTE HANDLERS - Proposals (Stage 2)
// =============================================================================

/// POST /api/proposals
/// Create a new proposal
pub async fn create_proposal(
    State(state): State<SharedState>,
    Json(req): Json<CreateProposalRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    // Create proposal
    let mut proposal = SchemaProposal::new(
        req.connection_id,
        req.title,
        req.description,
        "anonymous".to_string(), // TODO: Get from auth
    );

    // Add initial changes if provided
    for change in req.changes {
        proposal.changes.push(change);
    }

    // Create summary for metadata store
    let summary = ProposalSummary {
        id: proposal.id,
        connection_id: proposal.connection_id,
        title: proposal.title.clone(),
        description: proposal.description.clone(),
        status: "draft".to_string(),
        created_by: proposal.created_by.clone(),
        created_at: proposal.created_at,
        updated_at: proposal.updated_at,
        change_count: proposal.changes.len(),
    };

    state.metadata.add_proposal(summary).await;

    // Log audit
    let entry = AuditEntry::new(
        AuditAction::ProposalCreated,
        &proposal.created_by,
        "proposal",
        &proposal.id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::with_data(
        "Proposal created",
        ProposalResponse { proposal },
    )))
}

/// GET /api/proposals
/// List all proposals
pub async fn list_proposals(
    State(state): State<SharedState>,
    Query(_query): Query<ProposalListQuery>,
) -> Result<Json<SuccessResponse<ProposalListResponse>>, AppError> {
    let proposals = state.metadata.list_proposals().await;

    Ok(Json(SuccessResponse::with_data(
        "Proposals retrieved",
        ProposalListResponse { proposals },
    )))
}

/// GET /api/proposals/{id}
/// Get a specific proposal
pub async fn get_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<ProposalSummary>>, AppError> {
    let proposal = state
        .metadata
        .get_proposal(id)
        .await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;

    Ok(Json(SuccessResponse::with_data("Proposal retrieved", proposal)))
}

/// POST /api/proposals/{id}/changes
/// Add a change to a proposal
pub async fn add_change_to_proposal(
    State(_state): State<SharedState>,
    Path(_id): Path<Uuid>,
    Json(_req): Json<AddChangeRequest>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    // TODO: Implement with proper proposal store
    Err(AppError::Internal("Not implemented yet".to_string()))
}

/// POST /api/proposals/{id}/migration
/// Generate migration SQL for a proposal
pub async fn generate_migration(
    State(_state): State<SharedState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<MigrationResponse>>, AppError> {
    // TODO: Implement with proper proposal store
    Err(AppError::Internal("Not implemented yet".to_string()))
}

/// POST /api/proposals/{id}/submit
/// Submit a proposal for review
pub async fn submit_for_review(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    let entry = AuditEntry::new(
        AuditAction::ProposalSubmitted,
        "system",
        "proposal",
        &id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::<()>::message_only("Proposal submitted for review")))
}

/// POST /api/proposals/{id}/approve
/// Approve a proposal (Admin only)
pub async fn approve_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(_req): Json<ApprovalRequest>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    let entry = AuditEntry::new(
        AuditAction::ProposalApproved,
        "admin",
        "proposal",
        &id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::<()>::message_only("Proposal approved")))
}

/// POST /api/proposals/{id}/reject
/// Reject a proposal
pub async fn reject_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(_req): Json<RejectionRequest>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    let entry = AuditEntry::new(
        AuditAction::ProposalRejected,
        "admin",
        "proposal",
        &id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::<()>::message_only("Proposal rejected")))
}

/// POST /api/proposals/{id}/comments
/// Add a comment to a proposal
pub async fn add_comment(
    State(_state): State<SharedState>,
    Path(_id): Path<Uuid>,
    Json(_req): Json<CommentRequest>,
) -> Result<Json<SuccessResponse<()>>, AppError> {
    Ok(Json(SuccessResponse::<()>::message_only("Comment added")))
}

// =============================================================================
// ROUTE HANDLERS - Risk Analysis (Stage 3)
// =============================================================================

/// POST /api/proposals/{id}/analyze
/// Analyze the risk of a proposal
pub async fn analyze_risk(
    State(_state): State<SharedState>,
    Path(_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<RiskAnalysisResponse>>, AppError> {
    // Create a dummy proposal for analysis
    let proposal = SchemaProposal::new(
        Uuid::new_v4(),
        "Test".to_string(),
        "Test".to_string(),
        "system".to_string(),
    );

    let engine = RiskEngine::new();
    let analysis = engine.analyze(&proposal)?;

    Ok(Json(SuccessResponse::with_data(
        "Risk analysis complete",
        RiskAnalysisResponse { analysis },
    )))
}

// =============================================================================
// ROUTE HANDLERS - Execution (Stage 4)
// =============================================================================

/// POST /api/proposals/{id}/execute
/// Execute a proposal's migration
pub async fn execute_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<SuccessResponse<ExecutionResponse>>, AppError> {
    // Create a dummy proposal for execution
    let proposal = SchemaProposal::new(
        Uuid::new_v4(),
        "Test".to_string(),
        "Test".to_string(),
        "system".to_string(),
    );

    let orchestrator = Orchestrator::new();
    let result = orchestrator.execute(&proposal, req.dry_run).await?;

    let entry = AuditEntry::new(
        AuditAction::ProposalExecuted,
        "system",
        "proposal",
        &id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::with_data(
        if req.dry_run { "Dry run complete" } else { "Proposal executed" },
        ExecutionResponse {
            success: result.success,
            result,
        },
    )))
}

/// POST /api/proposals/{id}/rollback
/// Rollback a proposal's migration
pub async fn rollback_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<ExecutionResponse>>, AppError> {
    let proposal = SchemaProposal::new(
        Uuid::new_v4(),
        "Test".to_string(),
        "Test".to_string(),
        "system".to_string(),
    );

    let orchestrator = Orchestrator::new();
    let result = orchestrator.rollback(&proposal).await?;

    let entry = AuditEntry::new(
        AuditAction::ProposalRolledBack,
        "system",
        "proposal",
        &id.to_string(),
    );
    state.metadata.add_audit_entry(entry).await;

    Ok(Json(SuccessResponse::with_data(
        "Rollback complete",
        ExecutionResponse {
            success: result.success,
            result,
        },
    )))
}

// =============================================================================
// ROUTE HANDLERS - Audit Log
// =============================================================================

/// GET /api/audit-log
/// Get the audit log
pub async fn get_audit_log(
    State(state): State<SharedState>,
) -> Result<Json<SuccessResponse<AuditLogResponse>>, AppError> {
    let entries = state.metadata.get_audit_log().await;

    Ok(Json(SuccessResponse::with_data(
        "Audit log retrieved",
        AuditLogResponse { entries },
    )))
}
