//! Password hashing and verification
//!
//! Uses bcrypt for secure password hashing.

use crate::error::AppError;
use bcrypt::{hash, verify, DEFAULT_COST};

/// Hash a password using bcrypt
pub fn hash_password(password: &str) -> Result<String, AppError> {
    hash(password, DEFAULT_COST)
        .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
}

/// Verify a password against a hash
pub fn verify_password(password: &str, hash: &str) -> Result<bool, AppError> {
    verify(password, hash)
        .map_err(|e| AppError::Internal(format!("Failed to verify password: {}", e)))
}
