//! Application state management
//!
//! Contains shared state accessible across all handlers.
//! DATABASE-ONLY: All storage is backed by PostgreSQL, no in-memory fallbacks.

use crate::connection::ConnectionManager;
use crate::db::{UserService, ProjectService};
use crate::pipeline::MetadataStore;
use crate::proposal::ProposalStore;
use crate::snapshot::{SnapshotStore, RulesEngine};
use deadpool_postgres::Pool;
use std::sync::Arc;

/// Application state shared across all handlers
/// All operations require a valid database connection
pub struct AppState {
    /// Database connection pool (required)
    pub db_pool: Pool,
    
    /// User service for database operations (required)
    pub user_service: UserService,
    
    /// Project service for database operations (required)
    pub project_service: ProjectService,
    
    /// Dynamic connection manager for multi-database support
    pub connections: ConnectionManager,
    
    /// Governance Pipeline: Metadata store for proposals, snapshots, and audit logs
    pub metadata: MetadataStore,
    
    /// Proposal management store (has internal locking)
    pub proposals: ProposalStore,
    
    /// Schema snapshot store for versioned schema tracking
    pub snapshots: SnapshotStore,
    
    /// Rules engine for governance guardrails
    pub rules: RulesEngine,
    
    /// JWT secret key for token signing
    pub jwt_secret: String,
}

impl AppState {
    /// Create new application state with database pool (the only way)
    pub fn new(pool: Pool, jwt_secret: String) -> Self {
        let user_service = UserService::new(pool.clone());
        let project_service = ProjectService::new(pool.clone());
        
        Self {
            db_pool: pool,
            user_service,
            project_service,
            connections: ConnectionManager::new(),
            metadata: MetadataStore::new(),
            proposals: ProposalStore::new(),
            snapshots: SnapshotStore::new(),
            rules: RulesEngine::new(),
            jwt_secret,
        }
    }
}

/// Type alias for shared state
pub type SharedState = Arc<AppState>;
