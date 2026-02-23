//! Authentication route handlers
//!
//! Provides login, register, refresh, and user management endpoints.

use crate::auth::{
    create_tokens, decode_token, refresh_tokens, TokenPair,
    Role,
};
use crate::error::AppError;
use crate::state::SharedState;
use crate::users::User;
use axum::{
    extract::State,
    http::{header, StatusCode},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ============================================
// Request/Response Types
// ============================================

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub success: bool,
    pub user: UserResponse,
    pub tokens: TokenPair,
}

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: Role,
}

impl From<&User> for UserResponse {
    fn from(user: &User) -> Self {
        Self {
            id: user.id.to_string(),
            email: user.email.clone(),
            name: user.name.clone(),
            role: user.role,
        }
    }
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id.to_string(),
            email: user.email,
            name: user.name,
            role: user.role,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct TokenResponse {
    pub success: bool,
    pub tokens: TokenPair,
}

#[derive(Debug, Serialize)]
pub struct MeResponse {
    pub success: bool,
    pub user: UserResponse,
}

// ============================================
// Route Handlers
// ============================================

/// POST /api/auth/login
/// 
/// Authenticate with email and password, receive JWT tokens.
/// NOTE: Passwords are compared as plaintext (for testing only).
pub async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Use database service (required - no fallback)
    let db_user = state.user_service
        .find_by_email(&req.email)
        .await?
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".to_string()))?;
    
    // Verify password - PLAINTEXT comparison for testing
    if req.password != db_user.password_hash {
        return Err(AppError::Unauthorized("Invalid email or password".to_string()));
    }
    
    // Generate tokens
    let tokens = create_tokens(
        format!("{}", db_user.id),
        &db_user.email,
        Role::Viewer,
    )?;
    
    Ok(Json(AuthResponse {
        success: true,
        user: UserResponse {
            id: db_user.id.to_string(),
            email: db_user.email,
            name: db_user.name.unwrap_or_default(),
            role: Role::Viewer,
        },
        tokens,
    }))
}

/// POST /api/auth/register
/// 
/// Register a new user account. New users get Viewer role by default.
/// NOTE: Passwords are stored as plaintext (for testing only).
/// DATABASE ONLY - no in-memory fallbacks
pub async fn register(
    State(state): State<SharedState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    // Validate input
    if req.email.is_empty() || !req.email.contains('@') {
        return Err(AppError::BadRequest("Invalid email address".to_string()));
    }
    if req.password.len() < 6 {
        return Err(AppError::BadRequest("Password must be at least 6 characters".to_string()));
    }
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    
    // Create user in database (required - no fallback)
    let user = state.user_service
        .create_user(&req.email, &req.password, &req.name)
        .await?;
    
    // Generate tokens from database user
    let tokens = create_tokens(
        format!("{}", user.id),
        &user.email,
        Role::Viewer,
    )?;
    
    Ok((StatusCode::CREATED, Json(AuthResponse {
        success: true,
        user: UserResponse {
            id: user.id.to_string(),
            email: user.email,
            name: user.name.unwrap_or_default(),
            role: Role::Viewer,
        },
        tokens,
    })))
}

/// POST /api/auth/refresh
/// 
/// Refresh access token using refresh token.
pub async fn refresh(
    State(_state): State<SharedState>,
    Json(req): Json<RefreshRequest>,
) -> Result<Json<TokenResponse>, AppError> {
    let tokens = refresh_tokens(&req.refresh_token)?;
    
    Ok(Json(TokenResponse {
        success: true,
        tokens,
    }))
}

/// GET /api/auth/me
/// 
/// Get current user info from JWT token.
pub async fn me(
    State(state): State<SharedState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<MeResponse>, AppError> {
    // Extract token from Authorization header
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing authorization header".to_string()))?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Invalid authorization header format".to_string()))?;
    
    // Decode token
    let claims = decode_token(token)?;
    
    // Get user from database - claims.sub is the numeric user ID as string
    let user_id = claims.sub.parse::<i32>()
        .map_err(|_| AppError::Unauthorized("Invalid user ID in token".to_string()))?;
    
    let db_user = state.user_service
        .find_by_id(user_id)
        .await?
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;
    
    Ok(Json(MeResponse {
        success: true,
        user: UserResponse {
            id: db_user.id.to_string(),
            email: db_user.email,
            name: db_user.name.unwrap_or_default(),
            role: claims.role,
        },
    }))
}

/// PUT /api/auth/role/{user_id}
/// 
/// Update user role (Admin only).
#[derive(Debug, Deserialize)]
pub struct UpdateRoleRequest {
    pub role: Role,
}

pub async fn update_role(
    State(state): State<SharedState>,
    headers: axum::http::HeaderMap,
    axum::extract::Path(user_id): axum::extract::Path<String>,
    Json(req): Json<UpdateRoleRequest>,
) -> Result<Json<MeResponse>, AppError> {
    // Extract and verify admin token
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing authorization header".to_string()))?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Invalid authorization header format".to_string()))?;
    
    let claims = decode_token(token)?;
    
    // Check if requester is admin
    if claims.role != Role::Admin {
        return Err(AppError::Forbidden("Only admins can change user roles".to_string()));
    }
    
    // Parse user ID as i32
    let target_user_id = user_id.parse::<i32>()
        .map_err(|_| AppError::BadRequest("Invalid user ID format".to_string()))?;
    
    // Update user role in database
    let updated_user = state.user_service
        .update_role(target_user_id, &req.role.to_string())
        .await?
        .ok_or_else(|| AppError::NotFound("User not found".to_string()))?;
    
    Ok(Json(MeResponse {
        success: true,
        user: UserResponse {
            id: updated_user.id.to_string(),
            email: updated_user.email,
            name: updated_user.name.unwrap_or_default(),
            role: req.role,
        },
    }))
}

/// GET /api/users
/// 
/// List all users (Admin only).
#[derive(Debug, Serialize)]
pub struct UsersListResponse {
    pub success: bool,
    pub users: Vec<UserResponse>,
}

pub async fn list_users(
    State(state): State<SharedState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<UsersListResponse>, AppError> {
    // Extract and verify admin token
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing authorization header".to_string()))?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Invalid authorization header format".to_string()))?;
    
    let claims = decode_token(token)?;
    
    // Check if requester is admin
    if claims.role != Role::Admin {
        return Err(AppError::Forbidden("Only admins can list users".to_string()));
    }
    
    // Get all users from database
    let db_users = state.user_service.list_users().await?;
    let user_list: Vec<UserResponse> = db_users
        .into_iter()
        .map(|u| UserResponse {
            id: u.id.to_string(),
            email: u.email,
            name: u.name.unwrap_or_default(),
            role: claims.role.clone(),  // Note: All returned users get requester's role, ideally should be from DB
        })
        .collect();
    
    Ok(Json(UsersListResponse {
        success: true,
        users: user_list,
    }))
}
