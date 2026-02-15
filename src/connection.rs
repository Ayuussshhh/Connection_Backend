//! Database Connection Manager
//!
//! Handles multiple simultaneous database connections with dynamic connection strings.
//! This is the core of the "connect to any database" functionality.

use crate::error::AppError;
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_postgres::NoTls;
use tracing::{debug, info};
use uuid::Uuid;

/// Database type enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseType {
    Postgres,
    // MySQL,    // Future support
    // SQLite,   // Future support
}

impl DatabaseType {
    pub fn from_connection_string(conn_str: &str) -> Option<Self> {
        if conn_str.starts_with("postgres://") || conn_str.starts_with("postgresql://") {
            Some(DatabaseType::Postgres)
        } else {
            None
        }
    }
}

/// Environment classification for a database
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    Development,
    Staging,
    Production,
    Custom(String),
}

impl Default for Environment {
    fn default() -> Self {
        Environment::Development
    }
}

/// Connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Error(String),
}

/// Parsed connection parameters from a connection string
#[derive(Debug, Clone)]
pub struct ConnectionParams {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub db_type: DatabaseType,
}

impl ConnectionParams {
    /// Parse a PostgreSQL connection string
    /// Format: postgres://user:password@host:port/database
    pub fn from_connection_string(conn_str: &str) -> Result<Self, AppError> {
        let url = url::Url::parse(conn_str)
            .map_err(|e| AppError::Config(format!("Invalid connection string: {}", e)))?;

        let db_type = DatabaseType::from_connection_string(conn_str)
            .ok_or_else(|| AppError::Config("Unsupported database type. Use postgres://".to_string()))?;

        let host = url.host_str()
            .ok_or_else(|| AppError::Config("Missing host in connection string".to_string()))?
            .to_string();

        let port = url.port().unwrap_or(5432);

        let user = if url.username().is_empty() {
            "postgres".to_string()
        } else {
            url.username().to_string()
        };

        let password = url.password().unwrap_or("").to_string();

        let database = url.path().trim_start_matches('/').to_string();
        if database.is_empty() {
            return Err(AppError::Config("Missing database name in connection string".to_string()));
        }

        Ok(Self {
            host,
            port,
            user,
            password,
            database,
            db_type,
        })
    }

    /// Convert back to connection string (with password masked for display)
    #[allow(dead_code)]
    pub fn to_display_string(&self) -> String {
        format!(
            "postgres://{}:****@{}:{}/{}",
            self.user, self.host, self.port, self.database
        )
    }
}

/// A managed database connection
#[derive(Debug)]
pub struct ManagedConnection {
    pub id: Uuid,
    pub name: String,
    pub params: ConnectionParams,
    pub environment: Environment,
    pub status: ConnectionStatus,
    pub pool: Pool,
    pub connected_at: DateTime<Utc>,
    pub last_introspected_at: Option<DateTime<Utc>>,
}

/// Public connection info (safe to expose to frontend)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionInfo {
    pub id: Uuid,
    pub name: String,
    pub database: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub db_type: DatabaseType,
    pub environment: Environment,
    pub status: ConnectionStatus,
    pub connected_at: DateTime<Utc>,
    pub last_introspected_at: Option<DateTime<Utc>>,
}

impl From<&ManagedConnection> for ConnectionInfo {
    fn from(conn: &ManagedConnection) -> Self {
        Self {
            id: conn.id,
            name: conn.name.clone(),
            database: conn.params.database.clone(),
            host: conn.params.host.clone(),
            port: conn.params.port,
            user: conn.params.user.clone(),
            db_type: conn.params.db_type,
            environment: conn.environment.clone(),
            status: conn.status.clone(),
            connected_at: conn.connected_at,
            last_introspected_at: conn.last_introspected_at,
        }
    }
}

/// Connection Manager - handles multiple database connections
pub struct ConnectionManager {
    /// Active connections indexed by ID
    connections: RwLock<HashMap<Uuid, Arc<ManagedConnection>>>,
    
    /// Currently active/selected connection ID
    active_connection_id: RwLock<Option<Uuid>>,
    
    /// Default pool size for new connections
    #[allow(dead_code)]
    default_pool_size: usize,
}

impl ConnectionManager {
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            active_connection_id: RwLock::new(None),
            default_pool_size: 5,
        }
    }

    /// Create a new connection manager with custom pool size
    #[allow(dead_code)]
    pub fn with_pool_size(pool_size: usize) -> Self {
        Self {
            connections: RwLock::new(HashMap::new()),
            active_connection_id: RwLock::new(None),
            default_pool_size: pool_size,
        }
    }

    /// Connect to a database using a connection string
    pub async fn connect(
        &self,
        connection_string: &str,
        name: Option<String>,
        environment: Option<Environment>,
    ) -> Result<ConnectionInfo, AppError> {
        // Parse connection string
        let params = ConnectionParams::from_connection_string(connection_string)?;
        
        // Generate connection name if not provided
        let conn_name = name.unwrap_or_else(|| {
            format!("{}@{}", params.database, params.host)
        });

        // Create connection pool
        let pool = self.create_pool(&params)?;

        // Test connection
        let client = pool.get().await.map_err(|e| {
            AppError::Connection(format!("Failed to connect: {}", e))
        })?;
        
        // Verify connection works
        client.query_one("SELECT NOW()", &[]).await.map_err(|e| {
            AppError::Connection(format!("Connection test failed: {}", e))
        })?;
        drop(client);

        let conn_id = Uuid::new_v4();
        let now = Utc::now();

        let managed_conn = ManagedConnection {
            id: conn_id,
            name: conn_name,
            params,
            environment: environment.unwrap_or_default(),
            status: ConnectionStatus::Connected,
            pool,
            connected_at: now,
            last_introspected_at: None,
        };

        let conn_info = ConnectionInfo::from(&managed_conn);

        // Store connection
        {
            let mut connections = self.connections.write().await;
            connections.insert(conn_id, Arc::new(managed_conn));
        }

        // Set as active connection
        {
            let mut active = self.active_connection_id.write().await;
            *active = Some(conn_id);
        }

        info!("Connected to database: {} ({})", conn_info.database, conn_id);

        Ok(conn_info)
    }

    /// Create a connection pool for the given parameters
    fn create_pool(&self, params: &ConnectionParams) -> Result<Pool, AppError> {
        let mut cfg = Config::new();
        cfg.host = Some(params.host.clone());
        cfg.port = Some(params.port);
        cfg.user = Some(params.user.clone());
        cfg.password = Some(params.password.clone());
        cfg.dbname = Some(params.database.clone());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        cfg.create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| AppError::Config(format!("Failed to create pool: {}", e)))
    }

    /// Get a connection by ID
    pub async fn get_connection(&self, id: Uuid) -> Option<Arc<ManagedConnection>> {
        let connections = self.connections.read().await;
        connections.get(&id).cloned()
    }

    /// Get the currently active connection
    pub async fn get_active_connection(&self) -> Option<Arc<ManagedConnection>> {
        let active_id = self.active_connection_id.read().await;
        if let Some(id) = *active_id {
            self.get_connection(id).await
        } else {
            None
        }
    }

    /// Get active connection ID
    #[allow(dead_code)]
    pub async fn get_active_connection_id(&self) -> Option<Uuid> {
        *self.active_connection_id.read().await
    }

    /// Set the active connection
    pub async fn set_active_connection(&self, id: Uuid) -> Result<(), AppError> {
        let connections = self.connections.read().await;
        if !connections.contains_key(&id) {
            return Err(AppError::NotFound(format!("Connection {} not found", id)));
        }
        drop(connections);

        let mut active = self.active_connection_id.write().await;
        *active = Some(id);
        Ok(())
    }

    /// Get pool from a connection
    pub async fn get_pool(&self, id: Uuid) -> Result<Pool, AppError> {
        let conn = self.get_connection(id).await
            .ok_or_else(|| AppError::NotFound(format!("Connection {} not found", id)))?;
        Ok(conn.pool.clone())
    }

    /// Get pool from current active connection
    pub async fn get_active_pool(&self) -> Result<Pool, AppError> {
        let conn = self.get_active_connection().await
            .ok_or_else(|| AppError::NotConnected("No active database connection".to_string()))?;
        Ok(conn.pool.clone())
    }

    /// List all connections
    pub async fn list_connections(&self) -> Vec<ConnectionInfo> {
        let connections = self.connections.read().await;
        connections.values()
            .map(|c| ConnectionInfo::from(c.as_ref()))
            .collect()
    }

    /// Disconnect from a specific database
    pub async fn disconnect(&self, id: Uuid) -> Result<(), AppError> {
        let mut connections = self.connections.write().await;
        
        if connections.remove(&id).is_none() {
            return Err(AppError::NotFound(format!("Connection {} not found", id)));
        }

        drop(connections);

        // Clear active connection if it was this one
        {
            let mut active = self.active_connection_id.write().await;
            if *active == Some(id) {
                *active = None;
            }
        }

        info!("Disconnected from database: {}", id);
        Ok(())
    }

    /// Disconnect all connections
    pub async fn disconnect_all(&self) {
        let mut connections = self.connections.write().await;
        connections.clear();
        
        let mut active = self.active_connection_id.write().await;
        *active = None;
        
        info!("Disconnected from all databases");
    }

    /// Check if any connection exists
    #[allow(dead_code)]
    pub async fn has_connections(&self) -> bool {
        let connections = self.connections.read().await;
        !connections.is_empty()
    }

    /// Get connection count
    pub async fn connection_count(&self) -> usize {
        let connections = self.connections.read().await;
        connections.len()
    }

    /// Test a connection without adding it
    pub async fn test_connection(connection_string: &str) -> Result<ConnectionTestResult, AppError> {
        let params = ConnectionParams::from_connection_string(connection_string)?;
        
        let mut cfg = Config::new();
        cfg.host = Some(params.host.clone());
        cfg.port = Some(params.port);
        cfg.user = Some(params.user.clone());
        cfg.password = Some(params.password.clone());
        cfg.dbname = Some(params.database.clone());

        let pool = cfg.create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| AppError::Config(format!("Failed to create test pool: {}", e)))?;

        let start = std::time::Instant::now();
        
        let client = pool.get().await.map_err(|e| {
            AppError::Connection(format!("Failed to connect: {}", e))
        })?;

        // Get server version
        let row = client.query_one("SELECT version()", &[]).await?;
        let version: String = row.get(0);
        
        let latency = start.elapsed();

        Ok(ConnectionTestResult {
            success: true,
            latency_ms: latency.as_millis() as u64,
            server_version: version,
            database: params.database,
            host: params.host,
        })
    }

    /// Update last introspected timestamp for a connection
    #[allow(dead_code)]
    pub async fn update_introspected_at(&self, id: Uuid) -> Result<(), AppError> {
        let connections = self.connections.read().await;
        if let Some(_conn) = connections.get(&id) {
            // Note: In a real implementation, we'd need interior mutability here
            // For now, this is a placeholder
            debug!("Updated introspection timestamp for connection {}", id);
            Ok(())
        } else {
            Err(AppError::NotFound(format!("Connection {} not found", id)))
        }
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of testing a connection
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub success: bool,
    pub latency_ms: u64,
    pub server_version: String,
    pub database: String,
    pub host: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_connection_string() {
        let conn_str = "postgres://myuser:mypass@localhost:5432/mydb";
        let params = ConnectionParams::from_connection_string(conn_str).unwrap();
        
        assert_eq!(params.host, "localhost");
        assert_eq!(params.port, 5432);
        assert_eq!(params.user, "myuser");
        assert_eq!(params.password, "mypass");
        assert_eq!(params.database, "mydb");
        assert_eq!(params.db_type, DatabaseType::Postgres);
    }

    #[test]
    fn test_parse_connection_string_default_port() {
        let conn_str = "postgres://user:pass@host/db";
        let params = ConnectionParams::from_connection_string(conn_str).unwrap();
        
        assert_eq!(params.port, 5432);
    }

    #[test]
    fn test_parse_connection_string_postgresql_scheme() {
        let conn_str = "postgresql://user:pass@host:5433/db";
        let params = ConnectionParams::from_connection_string(conn_str).unwrap();
        
        assert_eq!(params.db_type, DatabaseType::Postgres);
        assert_eq!(params.port, 5433);
    }

    #[test]
    fn test_invalid_connection_string() {
        let result = ConnectionParams::from_connection_string("not a valid url");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_database() {
        let result = ConnectionParams::from_connection_string("postgres://user:pass@host/");
        assert!(result.is_err());
    }
}
