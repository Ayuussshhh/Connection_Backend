//! Authentication middleware
//!
//! Extracts and validates JWT tokens from requests.

use crate::auth::{Claims, Role, decode_token};
use crate::error::AppError;
use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use axum::http::header::AUTHORIZATION;

/// Extract claims from request
pub async fn auth_middleware(
    mut request: Request,
    next: Next,
) -> Result<Response, AppError> {
    let auth_header = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .ok_or_else(|| AppError::Unauthorized("Missing authorization header".to_string()))?;
    
    let token = auth_header
        .strip_prefix("Bearer ")
        .ok_or_else(|| AppError::Unauthorized("Invalid authorization format".to_string()))?;
    
    let claims = decode_token(token)?;
    
    // Insert claims into request extensions for handlers to use
    request.extensions_mut().insert(claims);
    
    Ok(next.run(request).await)
}

/// Require specific role
pub fn require_role(claims: &Claims, required: Role) -> Result<(), AppError> {
    let has_permission = match required {
        Role::Viewer => true, // Everyone can view
        Role::Developer => claims.role.can_propose(),
        Role::Admin => claims.role.can_approve(),
    };
    
    if !has_permission {
        return Err(AppError::Forbidden(format!(
            "Requires {} role, you have {}",
            required, claims.role
        )));
    }
    
    Ok(())
}
