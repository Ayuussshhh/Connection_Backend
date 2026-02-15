//! Pipeline Routes
//!
//! API endpoints for the Governance Pipeline.

use crate::error::AppError;
use crate::introspection::PostgresIntrospector;
use crate::models::SuccessResponse;
use crate::pipeline::metadata::{AuditAction, AuditEntry, ProposalSummary};
use crate::pipeline::mirror::{DriftCheckResult, MirrorService, SemanticMap};
use crate::pipeline::orchestrator::Orchestrator;
use crate::pipeline::proposal::{MigrationArtifacts, ProposalService, SchemaProposal};
use crate::pipeline::risk::RiskEngine;
use crate::pipeline::types::*;
use crate::state::SharedState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
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
    pub target: CommentTarget,
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
    pub result: DriftCheckResult,
}

// Dummy user for now (would come from auth in production)
fn current_user() -> (Uuid, String) {
    (Uuid::new_v4(), "Anonymous User".to_string())
}

// =============================================================================
// MIRROR ROUTES (Stage 1)
// =============================================================================

/// Build semantic map for a connection
pub async fn build_semantic_map(
    State(state): State<SharedState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<SemanticMapResponse>>, AppError> {
    let connection = state.connections.get_connection(connection_id).await
        .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", connection_id)))?;
    
    // Introspect schema
    let schema = PostgresIntrospector::introspect(&connection.pool, connection_id).await?;
    
    // Build semantic map
    let semantic_map = MirrorService::build_semantic_map(&connection.pool, connection_id, schema.clone()).await?;
    
    // Save to metadata store
    state.metadata.save_snapshot(schema).await?;
    state.metadata.save_semantic_map(semantic_map.clone()).await;
    
    Ok(Json(SuccessResponse::with_data(
        "Semantic map built successfully",
        SemanticMapResponse { semantic_map },
    )))
}

/// Check for schema drift
pub async fn check_drift(
    State(state): State<SharedState>,
    Path(connection_id): Path<Uuid>,
) -> Result<Json<SuccessResponse<DriftResponse>>, AppError> {
    let connection = state.connections.get_connection(connection_id).await
        .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", connection_id)))?;
    
    // Get stored checksum
    let snapshot = state.metadata.get_latest_snapshot(connection_id).await
        .ok_or_else(|| AppError::NotFound("No schema snapshot found. Please introspect first.".to_string()))?;
    
    let result = MirrorService::check_drift(&connection.pool, connection_id, &snapshot.checksum).await?;
    
    if result.has_drift {
        // Log drift detection
        state.metadata.log_audit(AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: None,
            action: AuditAction::SchemaDriftDetected,
            resource_type: "connection".to_string(),
            resource_id: Some(connection_id),
            details: Some(serde_json::json!({
                "old_checksum": result.stored_checksum,
                "new_checksum": result.live_checksum
            })),
            ip_address: None,
        }).await;
    }
    
    Ok(Json(SuccessResponse::with_data(
        if result.has_drift { "Schema drift detected!" } else { "No schema drift" },
        DriftResponse { result },
    )))
}

// =============================================================================
// PROPOSAL ROUTES (Stage 2)
// =============================================================================

/// Create a new proposal
pub async fn create_proposal(
    State(state): State<SharedState>,
    Json(req): Json<CreateProposalRequest>,
) -> Result<(StatusCode, Json<SuccessResponse<ProposalResponse>>), AppError> {
    let (user_id, user_name) = current_user();
    
    // Get base snapshot
    let snapshot = state.metadata.get_latest_snapshot(req.connection_id).await
        .ok_or_else(|| AppError::NotFound("No schema snapshot found. Please introspect first.".to_string()))?;
    
    let mut proposal = SchemaProposal::new(
        req.connection_id,
        snapshot.id,
        snapshot.checksum,
        user_id,
        user_name,
        req.title,
        req.description,
    );
    
    // Add initial changes if provided
    for change in req.changes {
        proposal.add_change(change);
    }
    
    let proposal = state.metadata.create_proposal(proposal).await?;
    
    Ok((
        StatusCode::CREATED,
        Json(SuccessResponse::with_data("Proposal created", ProposalResponse { proposal })),
    ))
}

/// Get a proposal by ID
pub async fn get_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    Ok(Json(SuccessResponse::with_data("Proposal retrieved", ProposalResponse { proposal })))
}

/// List proposals
pub async fn list_proposals(
    State(state): State<SharedState>,
    Query(query): Query<ProposalListQuery>,
) -> Result<Json<SuccessResponse<ProposalListResponse>>, AppError> {
    let status_filter = query.status.and_then(|s| match s.as_str() {
        "draft" => Some(ProposalStatus::Draft),
        "open" => Some(ProposalStatus::Open),
        "approved" => Some(ProposalStatus::Approved),
        "rejected" => Some(ProposalStatus::Rejected),
        "merged" => Some(ProposalStatus::Merged),
        "closed" => Some(ProposalStatus::Closed),
        _ => None,
    });
    
    let proposals = if let Some(conn_id) = query.connection_id {
        state.metadata.list_proposals(conn_id, status_filter).await
    } else {
        state.metadata.list_all_proposals(status_filter).await
    };
    
    Ok(Json(SuccessResponse::with_data(
        format!("Found {} proposals", proposals.len()),
        ProposalListResponse { proposals },
    )))
}

/// Add a change to a proposal
pub async fn add_change_to_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<AddChangeRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    if !proposal.status.can_edit() {
        return Err(AppError::Validation(format!(
            "Cannot edit proposal with status {:?}",
            proposal.status
        )));
    }
    
    proposal.add_change(req.change);
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data("Change added", ProposalResponse { proposal })))
}

/// Generate migration SQL for a proposal
pub async fn generate_migration(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<MigrationResponse>>, AppError> {
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    let migration = ProposalService::generate_migration(&proposal)?;
    
    // Cache the migration on the proposal
    proposal.migration = Some(migration.clone());
    state.metadata.update_proposal(proposal).await?;
    
    Ok(Json(SuccessResponse::with_data("Migration generated", MigrationResponse { migration })))
}

/// Submit proposal for review
pub async fn submit_for_review(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    proposal.submit_for_review()?;
    
    state.metadata.log_audit(AuditEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        user_id: Some(proposal.author_id),
        action: AuditAction::ProposalSubmitted,
        resource_type: "proposal".to_string(),
        resource_id: Some(id),
        details: None,
        ip_address: None,
    }).await;
    
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data("Proposal submitted for review", ProposalResponse { proposal })))
}

/// Approve a proposal
pub async fn approve_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ApprovalRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let (user_id, user_name) = current_user();
    
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    proposal.approve(user_id, user_name, req.comment)?;
    
    // Check if approval requirements are met
    let policy = ApprovalPolicy::default();
    let check = proposal.check_approval_requirements(&policy);
    
    if check.meets_requirements {
        proposal.mark_approved();
    }
    
    state.metadata.log_audit(AuditEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        user_id: Some(user_id),
        action: AuditAction::ProposalApproved,
        resource_type: "proposal".to_string(),
        resource_id: Some(id),
        details: Some(serde_json::json!({
            "approval_count": proposal.approvals.len(),
            "status": format!("{:?}", proposal.status)
        })),
        ip_address: None,
    }).await;
    
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data("Proposal approved", ProposalResponse { proposal })))
}

/// Reject a proposal
pub async fn reject_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<RejectionRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let (user_id, user_name) = current_user();
    
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    proposal.reject(user_id, user_name, req.reason)?;
    
    state.metadata.log_audit(AuditEntry {
        id: Uuid::new_v4(),
        timestamp: Utc::now(),
        user_id: Some(user_id),
        action: AuditAction::ProposalRejected,
        resource_type: "proposal".to_string(),
        resource_id: Some(id),
        details: None,
        ip_address: None,
    }).await;
    
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data("Proposal rejected", ProposalResponse { proposal })))
}

/// Add a comment to a proposal
pub async fn add_comment(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CommentRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let (user_id, user_name) = current_user();
    
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    let comment = Comment {
        id: Uuid::new_v4(),
        author_id: user_id,
        author_name: user_name,
        target: req.target,
        content: req.content,
        resolved: false,
        replies: vec![],
        created_at: Utc::now(),
        updated_at: None,
    };
    
    proposal.add_comment(comment);
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data("Comment added", ProposalResponse { proposal })))
}

// =============================================================================
// RISK ANALYSIS ROUTES (Stage 3)
// =============================================================================

/// Analyze risk for a proposal
pub async fn analyze_risk(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<RiskAnalysisResponse>>, AppError> {
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    // Get semantic map
    let semantic_map = state.metadata.get_semantic_map(proposal.connection_id).await
        .ok_or_else(|| AppError::NotFound("No semantic map found. Please build semantic map first.".to_string()))?;
    
    // Ensure migration is generated
    let migration = if let Some(ref m) = proposal.migration {
        m.clone()
    } else {
        let m = ProposalService::generate_migration(&proposal)?;
        proposal.migration = Some(m.clone());
        m
    };
    
    // Run risk analysis
    let analysis = RiskEngine::analyze(&proposal, &semantic_map, &migration).await?;
    
    // Cache analysis
    proposal.risk_analysis = Some(analysis.clone());
    state.metadata.update_proposal(proposal).await?;
    
    Ok(Json(SuccessResponse::with_data(
        format!("Risk analysis complete: {:.1}% safety score", analysis.safety_score),
        RiskAnalysisResponse { analysis },
    )))
}

// =============================================================================
// EXECUTION ROUTES (Stage 4)
// =============================================================================

/// Execute a proposal (apply migration)
pub async fn execute_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
    Json(req): Json<ExecuteRequest>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let (user_id, _) = current_user();
    
    let mut proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    // Get connection pool
    let connection = state.connections.get_connection(proposal.connection_id).await
        .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", proposal.connection_id)))?;
    
    // Execute
    let result = Orchestrator::execute(&connection.pool, &mut proposal, user_id, req.dry_run).await?;
    
    if !req.dry_run && result.success {
        proposal.mark_merged(result.clone());
        
        state.metadata.log_audit(AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: Some(user_id),
            action: AuditAction::ProposalExecuted,
            resource_type: "proposal".to_string(),
            resource_id: Some(id),
            details: Some(serde_json::json!({
                "duration_ms": result.duration_ms,
                "dry_run": req.dry_run
            })),
            ip_address: None,
        }).await;
    }
    
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data(
        if req.dry_run {
            if result.success { "Dry run successful" } else { "Dry run found issues" }
        } else {
            if result.success { "Migration executed successfully" } else { "Migration failed and was rolled back" }
        },
        ProposalResponse { proposal },
    )))
}

/// Rollback a previously executed proposal
pub async fn rollback_proposal(
    State(state): State<SharedState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SuccessResponse<ProposalResponse>>, AppError> {
    let (user_id, _) = current_user();
    
    let proposal = state.metadata.get_proposal(id).await
        .ok_or_else(|| AppError::NotFound(format!("Proposal {} not found", id)))?;
    
    // Get connection pool
    let connection = state.connections.get_connection(proposal.connection_id).await
        .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", proposal.connection_id)))?;
    
    // Execute rollback
    let result = Orchestrator::rollback(&connection.pool, &proposal, user_id).await?;
    
    let mut proposal = proposal;
    if result.success {
        proposal.execution_result = Some(result.clone());
        
        state.metadata.log_audit(AuditEntry {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            user_id: Some(user_id),
            action: AuditAction::ProposalRolledBack,
            resource_type: "proposal".to_string(),
            resource_id: Some(id),
            details: Some(serde_json::json!({
                "duration_ms": result.duration_ms
            })),
            ip_address: None,
        }).await;
    }
    
    state.metadata.update_proposal(proposal.clone()).await?;
    
    Ok(Json(SuccessResponse::with_data(
        if result.success { "Rollback successful" } else { "Rollback failed" },
        ProposalResponse { proposal },
    )))
}

// =============================================================================
// AUDIT LOG ROUTES
// =============================================================================

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditLogResponse {
    pub entries: Vec<AuditEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AuditLogQuery {
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

fn default_limit() -> usize {
    100
}

pub async fn get_audit_log(
    State(state): State<SharedState>,
    Query(query): Query<AuditLogQuery>,
) -> Result<Json<SuccessResponse<AuditLogResponse>>, AppError> {
    let entries = state.metadata.get_audit_log(
        query.resource_type.as_deref(),
        query.resource_id,
        query.limit,
    ).await;
    
    Ok(Json(SuccessResponse::with_data(
        format!("Retrieved {} audit entries", entries.len()),
        AuditLogResponse { entries },
    )))
}
