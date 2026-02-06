//! Application state management
//!
//! Contains shared state accessible across all handlers.

use crate::db::DatabaseManager;
use std::sync::Arc;

/// Application state shared across all handlers
pub struct AppState {
    /// Database manager instance
    pub db: DatabaseManager,
}

impl AppState {
    /// Create new application state
    pub fn new(db: DatabaseManager) -> Self {
        Self { db }
    }
}

/// Type alias for shared state
pub type SharedState = Arc<AppState>;
