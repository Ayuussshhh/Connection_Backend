//! Database connection management
//!
//! Handles connection pooling and database operations.

pub mod queries;

use crate::config::DatabaseConfig;
use crate::error::AppError;
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use tokio::sync::RwLock;
use tokio_postgres::NoTls;
use tracing::{debug, info};

/// Database connection configuration
#[derive(Debug, Clone)]
pub struct DbConnectionConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_pool_size: usize,
}

impl From<&DatabaseConfig> for DbConnectionConfig {
    fn from(config: &DatabaseConfig) -> Self {
        Self {
            host: config.host.clone(),
            port: config.port,
            user: config.user.clone(),
            password: config.password.clone(),
            database: config.database.clone(),
            max_pool_size: config.max_pool_size,
        }
    }
}

/// Connection info stored when connected to a specific database
#[derive(Debug, Clone)]
pub struct CurrentConnection {
    pub database: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub connected_at: chrono::DateTime<chrono::Utc>,
}

/// Database manager handling connection pools
pub struct DatabaseManager {
    /// Admin pool for database-level operations (CREATE/DROP DATABASE, etc.)
    admin_pool: Pool,
    
    /// Admin connection config
    admin_config: DbConnectionConfig,
    
    /// Current database pool (for table and FK operations)
    current_pool: RwLock<Option<Pool>>,
    
    /// Current connection info
    current_connection: RwLock<Option<CurrentConnection>>,
}

impl DatabaseManager {
    /// Create a new database manager
    pub async fn new(config: &DatabaseConfig) -> Result<Self, AppError> {
        let admin_config = DbConnectionConfig::from(config);
        let admin_pool = Self::create_pool(&admin_config)?;

        // Test connection
        let client = admin_pool.get().await?;
        client.query_one("SELECT 1", &[]).await?;
        drop(client);

        info!("Admin connection pool established");

        Ok(Self {
            admin_pool,
            admin_config,
            current_pool: RwLock::new(None),
            current_connection: RwLock::new(None),
        })
    }

    /// Create a connection pool with given configuration
    fn create_pool(config: &DbConnectionConfig) -> Result<Pool, AppError> {
        let mut cfg = Config::new();
        cfg.host = Some(config.host.clone());
        cfg.port = Some(config.port);
        cfg.user = Some(config.user.clone());
        cfg.password = Some(config.password.clone());
        cfg.dbname = Some(config.database.clone());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        cfg.create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| AppError::Config(format!("Failed to create pool: {}", e)))
    }

    /// Get admin pool for database-level operations
    pub fn admin_pool(&self) -> &Pool {
        &self.admin_pool
    }

    /// Get current database pool
    pub async fn current_pool(&self) -> Result<Pool, AppError> {
        let pool = self.current_pool.read().await;
        pool.clone()
            .ok_or_else(|| AppError::NotConnected("No database connected. Please connect first.".to_string()))
    }

    /// Get current connection info
    pub async fn connection_info(&self) -> Option<CurrentConnection> {
        self.current_connection.read().await.clone()
    }

    /// Check if connected to a database
    pub async fn is_connected(&self) -> bool {
        self.current_pool.read().await.is_some()
    }

    /// Connect to a specific database
    pub async fn connect(
        &self,
        database: &str,
        user: Option<&str>,
        password: Option<&str>,
        host: Option<&str>,
        port: Option<u16>,
    ) -> Result<CurrentConnection, AppError> {
        let config = DbConnectionConfig {
            host: host.map(String::from).unwrap_or_else(|| self.admin_config.host.clone()),
            port: port.unwrap_or(self.admin_config.port),
            user: user.map(String::from).unwrap_or_else(|| self.admin_config.user.clone()),
            password: password.map(String::from).unwrap_or_else(|| self.admin_config.password.clone()),
            database: database.to_string(),
            max_pool_size: self.admin_config.max_pool_size,
        };

        let pool = Self::create_pool(&config)?;
        
        // Test connection
        let client = pool.get().await?;
        client.query_one("SELECT NOW()", &[]).await?;
        drop(client);

        let connection_info = CurrentConnection {
            database: database.to_string(),
            host: config.host.clone(),
            port: config.port,
            user: config.user.clone(),
            connected_at: chrono::Utc::now(),
        };

        debug!("Connected to database: {}", database);

        // Update current pool and connection info
        {
            let mut pool_lock = self.current_pool.write().await;
            *pool_lock = Some(pool);
        }
        {
            let mut conn_lock = self.current_connection.write().await;
            *conn_lock = Some(connection_info.clone());
        }

        Ok(connection_info)
    }

    /// Disconnect from current database
    pub async fn disconnect(&self) {
        let mut pool_lock = self.current_pool.write().await;
        *pool_lock = None;
        
        let mut conn_lock = self.current_connection.write().await;
        *conn_lock = None;
        
        debug!("Disconnected from current database");
    }
}
