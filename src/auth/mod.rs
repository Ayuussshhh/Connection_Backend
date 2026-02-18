//! Authentication and authorization module
//!
//! Provides JWT-based authentication and role-based access control.

mod jwt;
mod middleware;
mod password;

pub use jwt::{Claims, TokenPair, create_tokens, decode_token, refresh_tokens};
pub use middleware::auth_middleware;
pub use password::{hash_password, verify_password};

use serde::{Deserialize, Serialize};

/// User roles for authorization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// Can view schemas but not propose changes
    Viewer,
    /// Can propose changes but not approve
    Developer,
    /// Can approve and execute changes
    Admin,
}

impl Role {
    pub fn can_propose(&self) -> bool {
        matches!(self, Role::Developer | Role::Admin)
    }

    pub fn can_approve(&self) -> bool {
        matches!(self, Role::Admin)
    }

    pub fn can_execute(&self) -> bool {
        matches!(self, Role::Admin)
    }
}

impl Default for Role {
    fn default() -> Self {
        Role::Viewer
    }
}

impl std::fmt::Display for Role {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Role::Viewer => write!(f, "viewer"),
            Role::Developer => write!(f, "developer"),
            Role::Admin => write!(f, "admin"),
        }
    }
}
