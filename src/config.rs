//! Application configuration module
//!
//! Handles loading and validating configuration from environment variables.

use serde::Deserialize;
use std::net::Ipv4Addr;
use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum ConfigError {
    #[error("Failed to load environment variables: {0}")]
    EnvLoad(#[from] dotenvy::Error),

    #[error("Missing required environment variable: {0}")]
    MissingVar(String),

    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),

    #[error("Failed to parse configuration: {0}")]
    ParseError(String),
}

/// Server configuration
#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: Ipv4Addr,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: Ipv4Addr::new(0, 0, 0, 0), // Bind to 0.0.0.0 for Railway/Docker
            port: 3000,
        }
    }
}

/// Database configuration
#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
    pub max_pool_size: usize,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 5432,
            user: "postgres".to_string(),
            password: String::new(),
            database: "postgres".to_string(),
            max_pool_size: 10,
        }
    }
}

/// CORS configuration
#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    pub allowed_origins: Vec<String>,
}

impl Default for CorsConfig {
    fn default() -> Self {
        Self {
            allowed_origins: vec!["http://localhost:3001".to_string()],
        }
    }
}

/// Complete application settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub cors: CorsConfig,
}

impl Settings {
    /// Load settings from environment variables
    pub fn load() -> Result<Self, ConfigError> {
        // Load .env file if it exists (ignore errors if file not found)
        let _ = dotenvy::dotenv();

        let server = ServerConfig {
            host: std::env::var("HOST")
                .ok()
                .and_then(|h| h.parse().ok())
                .unwrap_or_else(|| ServerConfig::default().host),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| ServerConfig::default().port),
        };

        // Try to load DATABASE_URL first (modern format), fall back to individual vars
        let database = if let Ok(database_url) = std::env::var("DATABASE_URL") {
            Self::parse_database_url(&database_url)?
        } else {
            // Fall back to individual environment variables
            DatabaseConfig {
                host: std::env::var("DB_HOST").unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("DB_PORT")
                    .ok()
                    .and_then(|p| p.parse().ok())
                    .unwrap_or(5432),
                user: std::env::var("DB_USER").unwrap_or_else(|_| "postgres".to_string()),
                password: std::env::var("DB_PASSWORD").unwrap_or_default(),
                database: std::env::var("DB_NAME").unwrap_or_else(|_| "postgres".to_string()),
                max_pool_size: std::env::var("DB_MAX_CONNECTIONS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(10),
            }
        };

        let cors = CorsConfig {
            allowed_origins: std::env::var("ALLOWED_ORIGINS")
                .ok()
                .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
                .unwrap_or_else(|| CorsConfig::default().allowed_origins),
        };

        Ok(Self {
            server,
            database,
            cors,
        })
    }

    /// Parse a DATABASE_URL connection string (postgresql://...)
    fn parse_database_url(url: &str) -> Result<DatabaseConfig, ConfigError> {
        match url::Url::parse(url) {
            Ok(parsed) => {
                let host = parsed.host_str()
                    .ok_or_else(|| ConfigError::InvalidValue("Missing host in DATABASE_URL".to_string()))?
                    .to_string();
                
                let port = parsed.port().unwrap_or(5432);
                
                let user = parsed.username().to_string();
                let password = parsed.password()
                    .map(|p| p.to_string())
                    .unwrap_or_default();
                
                let database = parsed.path()
                    .trim_start_matches('/')
                    .to_string();

                Ok(DatabaseConfig {
                    host,
                    port,
                    user,
                    password,
                    database,
                    max_pool_size: std::env::var("DB_MAX_CONNECTIONS")
                        .ok()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(10),
                })
            }
            Err(_) => Err(ConfigError::InvalidValue(
                "Invalid DATABASE_URL format (expected postgresql://...)".to_string()
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_server_config() {
        let config = ServerConfig::default();
        assert_eq!(config.host, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(config.port, 3000);
    }

    #[test]
    fn test_default_database_config() {
        let config = DatabaseConfig::default();
        assert_eq!(config.host, "localhost");
        assert_eq!(config.port, 5432);
    }
}
