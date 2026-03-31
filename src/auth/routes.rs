use argon2::{Argon2, PasswordHash, PasswordVerifier};
use axum::{
    extract::State,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use tera::Context;
use tower_sessions::Session;

use crate::{models::user::User, AppError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page))
        .route("/auth/login", post(login))
        .route("/auth/logout", post(logout))
}

// ── GET /login ───────────────────────────────────────────────────────────────

pub async fn login_page(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    // If already logged in, redirect to inventory
    if session.get::<i64>("user_id").await.ok().flatten().is_some() {
        return Ok(Redirect::to("/").into_response());
    }
    let ctx = Context::new();
    Ok(state.render("login.html", &ctx)?.into_response())
}

// ── POST /auth/login ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LoginForm {
    email: String,
    password: String,
}

pub async fn login(
    State(state): State<AppState>,
    session: Session,
    Form(form): Form<LoginForm>,
) -> Result<impl IntoResponse, AppError> {
    let user = sqlx::query_as!(
        User,
        "SELECT * FROM users WHERE email = ? AND is_active = 1",
        form.email
    )
    .fetch_optional(&state.db)
    .await?;

    let valid = verify_password(&form.password, user.as_ref());

    if !valid {
        let mut ctx = Context::new();
        ctx.insert("error", &true);
        return Ok(state.render("login.html", &ctx)?.into_response());
    }

    let user = user.unwrap();

    // Regenerate session ID on login to prevent session fixation
    session
        .cycle_id()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    session
        .insert("user_id", user.id)
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;

    // Track session_id → user_id for admin deactivation
    if let Some(sid) = session.id() {
        let sid_str = sid.to_string();
        sqlx::query!(
            "INSERT OR REPLACE INTO user_sessions (session_id, user_id) VALUES (?, ?)",
            sid_str, user.id
        )
        .execute(&state.db)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("Failed to track session: {e}");
            Default::default()
        });
    }

    Ok(Redirect::to("/").into_response())
}

// ── POST /auth/logout ─────────────────────────────────────────────────────────

pub async fn logout(
    State(state): State<AppState>,
    session: Session,
) -> Result<impl IntoResponse, AppError> {
    if let Some(sid) = session.id() {
        let sid_str = sid.to_string();
        sqlx::query!("DELETE FROM user_sessions WHERE session_id = ?", sid_str)
            .execute(&state.db)
            .await
            .ok();
    }
    session
        .flush()
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?;
    Ok(Redirect::to("/login"))
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Verify password in constant time.
/// When no user is found we still run a dummy verification so the response time
/// doesn't leak whether the email exists (prevents timing enumeration).
fn verify_password(password: &str, user: Option<&User>) -> bool {
    match user {
        Some(u) => {
            let Ok(hash) = PasswordHash::new(&u.password_hash) else {
                return false;
            };
            Argon2::default()
                .verify_password(password.as_bytes(), &hash)
                .is_ok()
        }
        None => {
            // Constant-time dummy: run argon2 verify against a known bad hash
            // so the caller can't infer "user not found" from a fast response.
            let dummy = "$argon2id$v=19$m=19456,t=2,p=1$c29tZXNhbHQ$RdescudvJCsgt3ub+b+dWRWJTmaaJObG";
            if let Ok(hash) = PasswordHash::new(dummy) {
                let _ = Argon2::default().verify_password(password.as_bytes(), &hash);
            }
            false
        }
    }
}

/// Render the login page as a plain `Html` response (used in tests).
pub async fn render_login(state: &AppState, error: bool) -> Result<Html<String>, AppError> {
    let mut ctx = Context::new();
    if error {
        ctx.insert("error", &true);
    }
    state.render("login.html", &ctx)
}
