use axum::extract::{FromRef, FromRequestParts, State};
use axum::http::request::Parts;
use tower_sessions::Session;

use crate::{models::user::User, AppError, AppState};

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

/// Authenticated user, extracted from the session on every protected request.
/// `branch_id` comes from the session/DB only — never from the request body or URL params.
/// If this extractor is missing from a handler signature, the handler won't compile,
/// making branch isolation a compiler-enforced guarantee.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: i64,
    pub role: Role,
    pub branch_id: i64,
    pub name: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == Role::Admin
    }
    pub fn is_coordinator(&self) -> bool {
        self.role == Role::Coordinator
    }
    pub fn is_sales_rep(&self) -> bool {
        self.role == Role::SalesRep
    }
}

impl<S> FromRequestParts<S> for AuthUser
where
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        // 1. Extract session (populated by SessionManagerLayer)
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Unauthorized)?;

        // 2. Get user_id from session data
        let user_id: i64 = session
            .get::<i64>("user_id")
            .await
            .map_err(|_| AppError::Unauthorized)?
            .ok_or(AppError::Unauthorized)?;

        // 3. Load AppState to query the DB
        let State(app_state) = State::<AppState>::from_request_parts(parts, state)
            .await
            .map_err(|_| AppError::Internal("app state missing".into()))?;

        // 4. Fetch the user — must be active, must exist
        let user = sqlx::query_as!(
            User,
            "SELECT * FROM users WHERE id = ? AND is_active = 1",
            user_id
        )
        .fetch_optional(&app_state.db)
        .await
        .map_err(AppError::Database)?
        .ok_or(AppError::Unauthorized)?;

        // 5. Parse role string — unknown roles are rejected
        let role = Role::from_str(&user.role).ok_or(AppError::Unauthorized)?;

        Ok(AuthUser {
            id: user.id,
            role,
            branch_id: user.branch_id,
            name: user.name,
        })
    }
}
