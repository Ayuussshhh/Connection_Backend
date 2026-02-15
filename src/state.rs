//! Application state management
//!
//! Contains shared state accessible across all handlers.

use crate::connection::ConnectionManager;
use crate::db::DatabaseManager;
use crate::pipeline::MetadataStore;
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
}

impl AppState {
    /// Create new application state with connection manager only (new way)
    pub fn new() -> Self {
        Self {
            connections: ConnectionManager::new(),
            db: None,
            metadata: MetadataStore::new(),
        }
    }
    
    /// Create new application state with legacy database manager
    pub fn with_legacy_db(db: DatabaseManager) -> Self {
        Self {
            connections: ConnectionManager::new(),
            db: Some(db),
            metadata: MetadataStore::new(),
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for shared state
pub type SharedState = Arc<AppState>;
