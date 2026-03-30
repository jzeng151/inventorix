// Lane A (Day 1): AuthUser extractor — compiler-enforced branch isolation.
// Every protected handler must accept `AuthUser` or it won't compile.
// branch_id comes from the session only — never from request body or URL params.

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::request::Parts,
};

use crate::AppError;

#[derive(Debug, Clone, PartialEq)]
pub enum Role {
    Admin,
    Coordinator,
    SalesRep,
}

impl Role {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "admin" => Some(Self::Admin),
            "coordinator" => Some(Self::Coordinator),
            "sales_rep" => Some(Self::SalesRep),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Coordinator => "coordinator",
            Self::SalesRep => "sales_rep",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub role: Role,
    pub branch_id: i64,
}

#[async_trait]
impl<S: Send + Sync> FromRequestParts<S> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // TODO (Lane A): extract session, look up user in DB, return AuthUser
        // For now, return Unauthorized so the type system is satisfied
        let _ = parts;
        Err(AppError::Unauthorized)
    }
}
