// Database service for user and project operations
//
// Provides direct database access for users and projects

use crate::error::AppError;
use deadpool_postgres::Pool;
use chrono::Utc;

// User record from database
#[derive(Clone, Debug)]
pub struct DbUser {
    pub id: i32,
    pub email: String,
    pub password_hash: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

// Project record from database
#[derive(Clone, Debug)]
pub struct DbProject {
    pub id: i32,
    pub owner_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub is_private: bool,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

// User service for database operations
pub struct UserService {
    pool: Pool,
}

impl UserService {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // Create a new user
    pub async fn create_user(&self, email: &str, password: &str, name: &str) -> Result<DbUser, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let now = Utc::now();
        let row = client.query_one(
            "INSERT INTO users (email, password_hash, name, created_at, updated_at) 
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, email, password_hash, name, avatar_url, created_at, updated_at",
            &[&email, &password, &name, &now, &now],
        )
        .await
        .map_err(|e| {
            if e.to_string().contains("unique constraint") {
                AppError::Conflict("Email already registered".to_string())
            } else {
                AppError::Internal(format!("Database error: {}", e))
            }
        })?;

        Ok(DbUser {
            id: row.get(0),
            email: row.get(1),
            password_hash: row.get(2),
            name: row.get(3),
            avatar_url: row.get(4),
            created_at: row.get(5),
            updated_at: row.get(6),
        })
    }

    // Find user by email
    pub async fn find_by_email(&self, email: &str) -> Result<Option<DbUser>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let row = client.query_opt(
            "SELECT id, email, password_hash, name, avatar_url, created_at, updated_at 
             FROM users WHERE email = $1",
            &[&email],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| DbUser {
            id: r.get(0),
            email: r.get(1),
            password_hash: r.get(2),
            name: r.get(3),
            avatar_url: r.get(4),
            created_at: r.get(5),
            updated_at: r.get(6),
        }))
    }

    // Find user by ID
    pub async fn find_by_id(&self, id: i32) -> Result<Option<DbUser>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let row = client.query_opt(
            "SELECT id, email, password_hash, name, avatar_url, created_at, updated_at 
             FROM users WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| DbUser {
            id: r.get(0),
            email: r.get(1),
            password_hash: r.get(2),
            name: r.get(3),
            avatar_url: r.get(4),
            created_at: r.get(5),
            updated_at: r.get(6),
        }))
    }

    // Update user role
    pub async fn update_role(&self, id: i32, role_name: &str) -> Result<Option<DbUser>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let now = Utc::now();
        let row = client.query_opt(
            "UPDATE users SET updated_at = $1 WHERE id = $2 
             RETURNING id, email, password_hash, name, avatar_url, created_at, updated_at",
            &[&now, &id],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| DbUser {
            id: r.get(0),
            email: r.get(1),
            password_hash: r.get(2),
            name: r.get(3),
            avatar_url: r.get(4),
            created_at: r.get(5),
            updated_at: r.get(6),
        }))
    }

    // List all users
    pub async fn list_users(&self) -> Result<Vec<DbUser>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let rows = client.query(
            "SELECT id, email, password_hash, name, avatar_url, created_at, updated_at 
             FROM users ORDER BY created_at DESC",
            &[],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(rows.into_iter().map(|r| DbUser {
            id: r.get(0),
            email: r.get(1),
            password_hash: r.get(2),
            name: r.get(3),
            avatar_url: r.get(4),
            created_at: r.get(5),
            updated_at: r.get(6),
        }).collect())
    }
}

// Project service for database operations
pub struct ProjectService {
    pool: Pool,
}

impl ProjectService {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // Create a new project
    pub async fn create_project(
        &self,
        owner_id: i32,
        name: &str,
        description: Option<&str>,
        icon: Option<&str>,
        color: Option<&str>,
    ) -> Result<DbProject, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let now = Utc::now();
        let row = client.query_one(
            "INSERT INTO projects (owner_id, name, description, icon, color, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, owner_id, name, description, icon, color, is_private, created_at, updated_at",
            &[&owner_id, &name, &description, &icon, &color, &now, &now],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(DbProject {
            id: row.get(0),
            owner_id: row.get(1),
            name: row.get(2),
            description: row.get(3),
            icon: row.get(4),
            color: row.get(5),
            is_private: row.get(6),
            created_at: row.get(7),
            updated_at: row.get(8),
        })
    }

    // Get projects for a user
    pub async fn list_by_user(&self, owner_id: i32) -> Result<Vec<DbProject>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let rows = client.query(
            "SELECT id, owner_id, name, description, icon, color, is_private, created_at, updated_at
             FROM projects WHERE owner_id = $1 ORDER BY created_at DESC",
            &[&owner_id],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(rows.into_iter().map(|r| DbProject {
            id: r.get(0),
            owner_id: r.get(1),
            name: r.get(2),
            description: r.get(3),
            icon: r.get(4),
            color: r.get(5),
            is_private: r.get(6),
            created_at: r.get(7),
            updated_at: r.get(8),
        }).collect())
    }

    // Get a specific project
    pub async fn get_by_id(&self, id: i32) -> Result<Option<DbProject>, AppError> {
        let client = self.pool.get().await
            .map_err(|e| AppError::Internal(format!("Database pool error: {}", e)))?;

        let row = client.query_opt(
            "SELECT id, owner_id, name, description, icon, color, is_private, created_at, updated_at
             FROM projects WHERE id = $1",
            &[&id],
        )
        .await
        .map_err(|e| AppError::Internal(format!("Database error: {}", e)))?;

        Ok(row.map(|r| DbProject {
            id: r.get(0),
            owner_id: r.get(1),
            name: r.get(2),
            description: r.get(3),
            icon: r.get(4),
            color: r.get(5),
            is_private: r.get(6),
            created_at: r.get(7),
            updated_at: r.get(8),
        }))
    }
}
