//! Application state management
//!
//! Contains shared state accessible across all handlers.

use crate::connection::ConnectionManager;
use crate::db::DatabaseManager;
use crate::pipeline::MetadataStore;
use crate::proposal::ProposalStore;
use crate::users::UserStore;
use std::sync::Arc;

/// Application state shared across all handlers
pub struct AppState {
    /// New: Dynamic connection manager for multi-database support
    pub connections: ConnectionManager,
    
    /// Legacy: Database manager instance (for backward compatibility)
    /// This is optional - server can start without .env database config
    pub db: Option<DatabaseManager>,
    
    /// Governance Pipeline: Metadata store for proposals, snapshots, and audit logs
    pub metadata: MetadataStore,
    
    /// User management store (has internal locking)
    pub users: UserStore,
    
    /// Proposal management store (has internal locking)
    pub proposals: ProposalStore,
    
    /// JWT secret key for token signing
    pub jwt_secret: String,
}

impl AppState {
    /// Create new application state with connection manager only (new way)
    pub fn new(jwt_secret: String) -> Self {
        Self {
            connections: ConnectionManager::new(),
            db: None,
            metadata: MetadataStore::new(),
            users: UserStore::new(),
            proposals: ProposalStore::new(),
            jwt_secret,
        }
    }
    
    /// Create new application state with legacy database manager
    pub fn with_legacy_db(db: DatabaseManager, jwt_secret: String) -> Self {
        Self {
            connections: ConnectionManager::new(),
            db: Some(db),
            metadata: MetadataStore::new(),
            users: UserStore::new(),
            proposals: ProposalStore::new(),
            jwt_secret,
        }
    }
}

/// Type alias for shared state
pub type SharedState = Arc<AppState>;
