//! Project and User models
//!
//! Represents database projects, users, and connections with multi-user support

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// User represents a registered user in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: i32,
    pub email: String,
    #[serde(skip_serializing)] // Never send password hash to client
    pub password_hash: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub is_active: bool,
    pub last_login: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Project represents a workspace/project that contains database connections
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: i32,
    pub owner_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub is_private: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// ProjectMember represents a user's access to a shared project
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectMember {
    pub id: i32,
    pub project_id: i32,
    pub user_id: i32,
    pub role: String, // "owner", "editor", "viewer"
    pub granted_at: DateTime<Utc>,
    pub granted_by: Option<i32>,
}

/// SavedConnection represents a database connection saved within a project
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedConnection {
    pub id: i32,
    pub project_id: i32,
    pub name: String,
    #[serde(skip_serializing)] // Never send encrypted string to client
    pub connection_string_encrypted: Vec<u8>, // Encrypted blob
    pub connection_type: String, // "postgres", "mysql", etc.
    pub environment: String, // "development", "staging", "production"
    pub is_active: bool,
    pub last_tested: Option<DateTime<Utc>>,
    pub test_status: Option<String>, // "success", "failed", "untested"
    pub created_by: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Request to register a new user
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub password_confirm: String,
    pub name: Option<String>,
}

/// Request to login
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Login response with JWT token
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoginResponse {
    pub user: UserResponse,
    pub access_token: String,
    pub token_type: String,
    pub expires_in: i32, // seconds
}

/// User response (safe to send to client)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: i32,
    pub email: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// Request to create a new project
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Request to update a project
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
}

/// Request to save a database connection (connection string is encrypted server-side)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConnectionRequest {
    pub name: String,
    pub connection_string: String, // Plain text, will be encrypted
    pub connection_type: String,
    pub environment: String,
}

/// Request to share project with another user
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShareProjectRequest {
    pub user_email: String,
    pub role: String, // "editor", "viewer"
}

/// Response for project with connection count
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWithStats {
    #[serde(flatten)]
    pub project: Project,
    pub connection_count: i64,
    pub member_count: i64,
    pub last_accessed: Option<DateTime<Utc>>,
}

/// Response for connection details (safe - no encrypted string)
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionResponse {
    pub id: i32,
    pub project_id: i32,
    pub name: String,
    pub connection_type: String,
    pub environment: String,
    pub is_active: bool,
    pub last_tested: Option<DateTime<Utc>>,
    pub test_status: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Alias for ConnectionDetails (backward compatibility)
pub type ConnectionDetails = ConnectionResponse;

/// Response for project list
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListResponse {
    pub projects: Vec<ProjectWithStats>,
    pub total: i64,
}
