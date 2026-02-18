//! User management module
//!
//! Handles user storage and retrieval.

use crate::auth::Role;
use crate::error::AppError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// User model
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub name: String,
    pub role: Role,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// User response (without sensitive data)
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserResponse {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: Role,
    pub avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        UserResponse {
            id: user.id,
            email: user.email,
            name: user.name,
            role: user.role,
            avatar_url: user.avatar_url,
            created_at: user.created_at,
        }
    }
}

/// In-memory user store
pub struct UserStore {
    users: Arc<RwLock<HashMap<Uuid, User>>>,
    email_index: Arc<RwLock<HashMap<String, Uuid>>>,
}

impl UserStore {
    pub fn new() -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            email_index: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a new user
    pub async fn create(&self, user: User) -> Result<User, AppError> {
        let mut users = self.users.write().await;
        let mut email_index = self.email_index.write().await;
        
        // Check if email already exists
        if email_index.contains_key(&user.email) {
            return Err(AppError::Conflict("Email already registered".to_string()));
        }
        
        email_index.insert(user.email.clone(), user.id);
        users.insert(user.id, user.clone());
        
        Ok(user)
    }

    /// Find user by email
    pub async fn find_by_email(&self, email: &str) -> Option<User> {
        let email_index = self.email_index.read().await;
        let users = self.users.read().await;
        
        email_index.get(email).and_then(|id| users.get(id).cloned())
    }

    /// Find user by ID
    pub async fn find_by_id(&self, id: Uuid) -> Option<User> {
        let users = self.users.read().await;
        users.get(&id).cloned()
    }

    /// List all users
    pub async fn list(&self) -> Vec<UserResponse> {
        let users = self.users.read().await;
        users.values().cloned().map(UserResponse::from).collect()
    }

    /// Update user
    pub async fn update(&self, id: Uuid, updates: UserUpdate) -> Result<User, AppError> {
        let mut users = self.users.write().await;
        
        let user = users
            .get_mut(&id)
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
        
        if let Some(name) = updates.name {
            user.name = name;
        }
        if let Some(role) = updates.role {
            user.role = role;
        }
        if let Some(avatar_url) = updates.avatar_url {
            user.avatar_url = Some(avatar_url);
        }
        
        user.updated_at = Utc::now();
        
        Ok(user.clone())
    }

    /// Delete user
    pub async fn delete(&self, id: Uuid) -> Result<(), AppError> {
        let mut users = self.users.write().await;
        let mut email_index = self.email_index.write().await;
        
        let user = users
            .remove(&id)
            .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
        
        email_index.remove(&user.email);
        
        Ok(())
    }

    /// Initialize with default admin user
    pub async fn init_default_admin(&self) -> Result<(), AppError> {
        use crate::auth::hash_password;
        
        let admin = User {
            id: Uuid::new_v4(),
            email: "admin@schemaflow.local".to_string(),
            password_hash: hash_password("admin123")?,
            name: "Admin".to_string(),
            role: Role::Admin,
            avatar_url: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        
        // Ignore error if already exists
        let _ = self.create(admin).await;
        
        Ok(())
    }
}

impl Default for UserStore {
    fn default() -> Self {
        Self::new()
    }
}

/// User update payload
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserUpdate {
    pub name: Option<String>,
    pub role: Option<Role>,
    pub avatar_url: Option<String>,
}
