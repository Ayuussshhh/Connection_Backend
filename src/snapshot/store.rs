//! Schema Snapshot Store
//!
//! Manages versioned schema snapshots for comparison and auditing.
//! Think of this as "git commits" for your database schema.

use crate::error::AppError;
use crate::introspection::SchemaSnapshot;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Metadata about a snapshot (lightweight, used for listing)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub id: Uuid,
    pub connection_id: Uuid,
    pub version: u64,
    pub captured_at: DateTime<Utc>,
    pub checksum: String,
    pub table_count: usize,
    pub fk_count: usize,
    pub index_count: usize,
    /// Optional label for this snapshot (e.g., "before migration v2.3")
    pub label: Option<String>,
    /// Who captured this snapshot
    pub captured_by: Option<Uuid>,
}

impl From<&SchemaSnapshot> for SnapshotMetadata {
    fn from(snapshot: &SchemaSnapshot) -> Self {
        Self {
            id: snapshot.id,
            connection_id: snapshot.connection_id,
            version: snapshot.version,
            captured_at: snapshot.captured_at,
            checksum: snapshot.checksum.clone(),
            table_count: snapshot.tables.len(),
            fk_count: snapshot.foreign_keys.len(),
            index_count: snapshot.indexes.len(),
            label: None,
            captured_by: None,
        }
    }
}

/// Store for managing schema snapshots
pub struct SnapshotStore {
    /// Connection ID -> (Version -> Snapshot)
    snapshots: Arc<RwLock<HashMap<Uuid, HashMap<u64, SchemaSnapshot>>>>,
    /// Connection ID -> Latest version number
    versions: Arc<RwLock<HashMap<Uuid, u64>>>,
    /// Connection ID -> Baseline snapshot ID (the "production" state)
    baselines: Arc<RwLock<HashMap<Uuid, Uuid>>>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self {
            snapshots: Arc::new(RwLock::new(HashMap::new())),
            versions: Arc::new(RwLock::new(HashMap::new())),
            baselines: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Store a new snapshot, auto-incrementing version
    pub async fn save(&self, mut snapshot: SchemaSnapshot) -> Result<SchemaSnapshot, AppError> {
        let connection_id = snapshot.connection_id;
        
        // Get next version number
        let mut versions = self.versions.write().await;
        let current_version = versions.get(&connection_id).copied().unwrap_or(0);
        let new_version = current_version + 1;
        
        snapshot.version = new_version;
        versions.insert(connection_id, new_version);
        
        // Store the snapshot
        let mut snapshots = self.snapshots.write().await;
        let connection_snapshots = snapshots
            .entry(connection_id)
            .or_insert_with(HashMap::new);
        connection_snapshots.insert(new_version, snapshot.clone());
        
        tracing::info!(
            "Saved snapshot v{} for connection {}: {} tables, {} FKs",
            new_version,
            connection_id,
            snapshot.tables.len(),
            snapshot.foreign_keys.len()
        );
        
        Ok(snapshot)
    }

    /// Get the latest snapshot for a connection
    pub async fn get_latest(&self, connection_id: Uuid) -> Option<SchemaSnapshot> {
        let versions = self.versions.read().await;
        let version = versions.get(&connection_id)?;
        
        let snapshots = self.snapshots.read().await;
        snapshots
            .get(&connection_id)?
            .get(version)
            .cloned()
    }

    /// Get a specific version
    pub async fn get_version(&self, connection_id: Uuid, version: u64) -> Option<SchemaSnapshot> {
        let snapshots = self.snapshots.read().await;
        snapshots
            .get(&connection_id)?
            .get(&version)
            .cloned()
    }

    /// Get snapshot by ID
    pub async fn get_by_id(&self, snapshot_id: Uuid) -> Option<SchemaSnapshot> {
        let snapshots = self.snapshots.read().await;
        for connection_snapshots in snapshots.values() {
            for snapshot in connection_snapshots.values() {
                if snapshot.id == snapshot_id {
                    return Some(snapshot.clone());
                }
            }
        }
        None
    }

    /// List all snapshots for a connection (metadata only)
    pub async fn list(&self, connection_id: Uuid) -> Vec<SnapshotMetadata> {
        let snapshots = self.snapshots.read().await;
        
        snapshots
            .get(&connection_id)
            .map(|m| {
                let mut list: Vec<_> = m.values()
                    .map(SnapshotMetadata::from)
                    .collect();
                list.sort_by(|a, b| b.version.cmp(&a.version));
                list
            })
            .unwrap_or_default()
    }

    /// Set baseline snapshot (the "production" reference)
    pub async fn set_baseline(&self, connection_id: Uuid, snapshot_id: Uuid) -> Result<(), AppError> {
        // Verify snapshot exists
        if self.get_by_id(snapshot_id).await.is_none() {
            return Err(AppError::NotFound("Snapshot not found".to_string()));
        }
        
        let mut baselines = self.baselines.write().await;
        baselines.insert(connection_id, snapshot_id);
        
        tracing::info!("Set baseline for connection {} to snapshot {}", connection_id, snapshot_id);
        Ok(())
    }

    /// Get baseline snapshot for a connection
    pub async fn get_baseline(&self, connection_id: Uuid) -> Option<SchemaSnapshot> {
        let baselines = self.baselines.read().await;
        let baseline_id = baselines.get(&connection_id)?;
        self.get_by_id(*baseline_id).await
    }

    /// Delete old snapshots, keeping the last N versions
    pub async fn prune(&self, connection_id: Uuid, keep_versions: usize) -> Result<usize, AppError> {
        let mut snapshots = self.snapshots.write().await;
        
        if let Some(connection_snapshots) = snapshots.get_mut(&connection_id) {
            if connection_snapshots.len() <= keep_versions {
                return Ok(0);
            }
            
            // Get versions sorted descending
            let mut versions: Vec<_> = connection_snapshots.keys().copied().collect();
            versions.sort_by(|a, b| b.cmp(a));
            
            // Remove old versions
            let to_remove: Vec<_> = versions.into_iter().skip(keep_versions).collect();
            let removed_count = to_remove.len();
            
            for v in to_remove {
                connection_snapshots.remove(&v);
            }
            
            tracing::info!("Pruned {} old snapshots for connection {}", removed_count, connection_id);
            Ok(removed_count)
        } else {
            Ok(0)
        }
    }

    /// Compare two snapshots by version number
    pub async fn compare_versions(
        &self,
        connection_id: Uuid,
        from_version: u64,
        to_version: u64,
    ) -> Result<(SchemaSnapshot, SchemaSnapshot), AppError> {
        let from = self
            .get_version(connection_id, from_version)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Snapshot v{} not found", from_version)))?;
        
        let to = self
            .get_version(connection_id, to_version)
            .await
            .ok_or_else(|| AppError::NotFound(format!("Snapshot v{} not found", to_version)))?;
        
        Ok((from, to))
    }
}

impl Default for SnapshotStore {
    fn default() -> Self {
        Self::new()
    }
}
