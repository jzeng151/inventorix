use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use tera::Context;

use crate::{auth::extractor::{AuthUser, Role}, AppError, AppState};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/admin", get(admin_page))
        .route("/admin/users", post(create_user))
        .route("/admin/users/{id}/deactivate", post(deactivate_user))
        .route("/admin/users/{id}/reactivate", post(reactivate_user))
}

// ── GET /admin ────────────────────────────────────────────────────────────────

async fn admin_page(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    let users = sqlx::query!(
        r#"
        SELECT u.id, u.name, u.email, u.role, u.is_active, u.created_at,
               b.name AS branch_name
        FROM users u
        JOIN branches b ON u.branch_id = b.id
        ORDER BY u.is_active DESC, u.name ASC
        "#
    )
    .fetch_all(&state.db)
    .await?;

    let branches = sqlx::query!("SELECT id, name FROM branches ORDER BY name")
        .fetch_all(&state.db)
        .await?;

    #[derive(serde::Serialize)]
    struct UserRow {
        id: i64,
        name: String,
        email: String,
        role: String,
        is_active: bool,
        created_at: String,
        branch_name: String,
    }

    #[derive(serde::Serialize)]
    struct BranchOption {
        id: i64,
        name: String,
    }

    let user_rows: Vec<UserRow> = users
        .into_iter()
        .map(|u| UserRow {
            id: u.id,
            name: u.name,
            email: u.email,
            role: u.role,
            is_active: u.is_active != 0,
            created_at: u.created_at,
            branch_name: u.branch_name,
        })
        .collect();

    let branch_options: Vec<BranchOption> = branches
        .into_iter()
        .map(|b| BranchOption { id: b.id, name: b.name })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("users", &user_rows);
    ctx.insert("branches", &branch_options);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", "Admin Panel");

    state.render("admin/page.html", &ctx)
}

// ── POST /admin/users ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct CreateUserForm {
    name: String,
    email: String,
    password: String,
    role: String,
    branch_id: i64,
}

async fn create_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Form(form): Form<CreateUserForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    if !["admin", "coordinator", "sales_rep"].contains(&form.role.as_str()) {
        return Err(AppError::ValidationError(format!("Invalid role '{}'", form.role)));
    }
    if form.name.trim().is_empty() || form.email.trim().is_empty() || form.password.is_empty() {
        return Err(AppError::ValidationError(
            "Name, email, and password are required".into(),
        ));
    }

    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(form.password.as_bytes(), &salt)
        .map_err(|e| AppError::Internal(format!("Hash error: {e}")))?
        .to_string();

    sqlx::query!(
        "INSERT INTO users (branch_id, name, role, email, password_hash) VALUES (?, ?, ?, ?, ?)",
        form.branch_id, form.name, form.role, form.email, hash,
    )
    .execute(&state.db)
    .await
    .map_err(|e| {
        if e.to_string().contains("UNIQUE") {
            AppError::Conflict(format!("Email '{}' is already in use", form.email))
        } else {
            AppError::Database(e)
        }
    })?;

    tracing::info!("Admin {} created user '{}'", auth.name, form.email);
    Ok(Redirect::to("/admin"))
}

// ── POST /admin/users/{id}/deactivate ─────────────────────────────────────────

async fn deactivate_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    if user_id == auth.id {
        return Err(AppError::ValidationError(
            "You cannot deactivate your own account".into(),
        ));
    }

    sqlx::query!("UPDATE users SET is_active = 0 WHERE id = ?", user_id)
        .execute(&state.db)
        .await?;

    // Delete sessions from tower_sessions and our tracking table
    sqlx::query!(
        "DELETE FROM tower_sessions WHERE id IN \
         (SELECT session_id FROM user_sessions WHERE user_id = ?)",
        user_id
    )
    .execute(&state.db)
    .await?;

    sqlx::query!("DELETE FROM user_sessions WHERE user_id = ?", user_id)
        .execute(&state.db)
        .await?;

    tracing::info!("Admin {} deactivated user {}", auth.name, user_id);
    Ok(Redirect::to("/admin"))
}

// ── POST /admin/users/{id}/reactivate ────────────────────────────────────────

async fn reactivate_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(user_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    sqlx::query!("UPDATE users SET is_active = 1 WHERE id = ?", user_id)
        .execute(&state.db)
        .await?;

    tracing::info!("Admin {} reactivated user {}", auth.name, user_id);
    Ok(Redirect::to("/admin"))
}
