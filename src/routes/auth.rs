//! Authentication route handlers
//!
//! Provides login, register, refresh, and user management endpoints.

use crate::auth::{
    create_tokens, decode_token, refresh_tokens, TokenPair,
    hash_password, verify_password,
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
pub async fn login(
    State(state): State<SharedState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, AppError> {
    // Find user by email
    let user = state.users
        .find_by_email(&req.email)
        .await
        .ok_or_else(|| AppError::Unauthorized("Invalid email or password".to_string()))?;
    
    // Verify password
    if !verify_password(&req.password, &user.password_hash)? {
        return Err(AppError::Unauthorized("Invalid email or password".to_string()));
    }
    
    // Generate tokens
    let tokens = create_tokens(user.id, &user.email, user.role)?;
    
    Ok(Json(AuthResponse {
        success: true,
        user: UserResponse::from(&user),
        tokens,
    }))
}

/// POST /api/auth/register
/// 
/// Register a new user account. New users get Viewer role by default.
pub async fn register(
    State(state): State<SharedState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    // Validate input
    if req.email.is_empty() || !req.email.contains('@') {
        return Err(AppError::BadRequest("Invalid email address".to_string()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest("Password must be at least 8 characters".to_string()));
    }
    if req.name.is_empty() {
        return Err(AppError::BadRequest("Name is required".to_string()));
    }
    
    // Check if email already exists
    if state.users.find_by_email(&req.email).await.is_some() {
        return Err(AppError::Conflict("Email already registered".to_string()));
    }
    
    // Hash password
    let password_hash = hash_password(&req.password)?;
    
    // Create user with Viewer role by default
    let now = Utc::now();
    let user = User {
        id: Uuid::new_v4(),
        email: req.email,
        password_hash,
        name: req.name,
        role: Role::Viewer,
        avatar_url: None,
        created_at: now,
        updated_at: now,
    };
    
    let created_user = state.users.create(user).await?;
    
    // Generate tokens
    let tokens = create_tokens(created_user.id, &created_user.email, created_user.role)?;
    
    Ok((StatusCode::CREATED, Json(AuthResponse {
        success: true,
        user: UserResponse::from(&created_user),
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
    
    // Get user from store - claims.sub is already a Uuid
    let user = state.users
        .find_by_id(claims.sub)
        .await
        .ok_or_else(|| AppError::Unauthorized("User not found".to_string()))?;
    
    Ok(Json(MeResponse {
        success: true,
        user: UserResponse::from(&user),
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
    
    // Parse user ID
    let target_user_id = Uuid::parse_str(&user_id)
        .map_err(|_| AppError::BadRequest("Invalid user ID format".to_string()))?;
    
    // Update user role
    let updates = crate::users::UserUpdate {
        name: None,
        role: Some(req.role),
        avatar_url: None,
    };
    
    let updated_user = state.users.update(target_user_id, updates).await?;
    
    Ok(Json(MeResponse {
        success: true,
        user: UserResponse::from(&updated_user),
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
    
    let user_responses = state.users.list().await;
    let user_list: Vec<UserResponse> = user_responses
        .into_iter()
        .map(|ur| UserResponse {
            id: ur.id.to_string(),
            email: ur.email,
            name: ur.name,
            role: ur.role,
        })
        .collect();
    
    Ok(Json(UsersListResponse {
        success: true,
        users: user_list,
    }))
}
