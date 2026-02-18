//! JWT token management
//!
//! Handles creation, validation, and refresh of JWT tokens.

use crate::auth::Role;
use crate::error::AppError;
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// JWT secret key (should be from environment in production)
static JWT_SECRET: Lazy<String> = Lazy::new(|| {
    std::env::var("JWT_SECRET").unwrap_or_else(|_| {
        "schemaflow-dev-secret-key-change-in-production".to_string()
    })
});

/// Access token expiration (15 minutes)
const ACCESS_TOKEN_EXPIRATION_MINUTES: i64 = 15;

/// Refresh token expiration (7 days)
const REFRESH_TOKEN_EXPIRATION_DAYS: i64 = 7;

/// JWT claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// Subject (user ID)
    pub sub: Uuid,
    /// User email
    pub email: String,
    /// User role
    pub role: Role,
    /// Expiration time (Unix timestamp)
    pub exp: i64,
    /// Issued at (Unix timestamp)
    pub iat: i64,
    /// Token type (access or refresh)
    pub token_type: TokenType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

/// Token pair response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// Create access and refresh tokens for a user
pub fn create_tokens(user_id: Uuid, email: &str, role: Role) -> Result<TokenPair, AppError> {
    let now = Utc::now();
    
    // Create access token
    let access_claims = Claims {
        sub: user_id,
        email: email.to_string(),
        role,
        exp: (now + Duration::minutes(ACCESS_TOKEN_EXPIRATION_MINUTES)).timestamp(),
        iat: now.timestamp(),
        token_type: TokenType::Access,
    };
    
    let access_token = encode(
        &Header::default(),
        &access_claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    ).map_err(|e| AppError::Internal(format!("Failed to create access token: {}", e)))?;
    
    // Create refresh token
    let refresh_claims = Claims {
        sub: user_id,
        email: email.to_string(),
        role,
        exp: (now + Duration::days(REFRESH_TOKEN_EXPIRATION_DAYS)).timestamp(),
        iat: now.timestamp(),
        token_type: TokenType::Refresh,
    };
    
    let refresh_token = encode(
        &Header::default(),
        &refresh_claims,
        &EncodingKey::from_secret(JWT_SECRET.as_bytes()),
    ).map_err(|e| AppError::Internal(format!("Failed to create refresh token: {}", e)))?;
    
    Ok(TokenPair {
        access_token,
        refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: ACCESS_TOKEN_EXPIRATION_MINUTES * 60,
    })
}

/// Decode and validate a JWT token
pub fn decode_token(token: &str) -> Result<Claims, AppError> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(JWT_SECRET.as_bytes()),
        &Validation::default(),
    ).map_err(|e| match e.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
            AppError::Unauthorized("Token expired".to_string())
        }
        jsonwebtoken::errors::ErrorKind::InvalidToken => {
            AppError::Unauthorized("Invalid token".to_string())
        }
        _ => AppError::Unauthorized(format!("Token validation failed: {}", e)),
    })?;
    
    Ok(token_data.claims)
}

/// Refresh tokens using a valid refresh token
pub fn refresh_tokens(refresh_token: &str) -> Result<TokenPair, AppError> {
    let claims = decode_token(refresh_token)?;
    
    if claims.token_type != TokenType::Refresh {
        return Err(AppError::Unauthorized("Invalid token type for refresh".to_string()));
    }
    
    create_tokens(claims.sub, &claims.email, claims.role)
}
