use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::post,
    Form, Router,
};
use serde::Deserialize;

use crate::{
    auth::extractor::{AuthUser, Role},
    salesforce::client::RefillPayload,
    ws::manager::WsEvent,
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/tiles/{id}/refill", post(request_refill))
        .route("/refill/{id}/approve", post(approve_refill))
        .route("/refill/{id}/reject", post(reject_refill))
        .route("/refill/{id}/fulfill", post(fulfill_refill))
}

// ── POST /tiles/{id}/refill ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct RefillForm {
    qty: i64,
}

async fn request_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tile_id): Path<i64>,
    Form(form): Form<RefillForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }
    if form.qty <= 0 || form.qty > 10_000 {
        return Err(AppError::ValidationError(
            "Qty must be between 1 and 10,000".into(),
        ));
    }

    // Branch isolation — also fetch item_number for the WS notification
    let tile = sqlx::query!(
        "SELECT id, item_number FROM tiles WHERE id = ? AND branch_id = ?",
        tile_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    // Block duplicate pending requests (approved ones are fine — coordinator may need more)
    if sqlx::query!(
        "SELECT id FROM refill_requests WHERE tile_id = ? AND status = 'pending'",
        tile_id
    )
    .fetch_optional(&state.db)
    .await?
    .is_some()
    {
        return Err(AppError::Conflict(
            "A pending refill request already exists for this tile".into(),
        ));
    }

    let refill_id = sqlx::query!(
        r#"
        INSERT INTO refill_requests (tile_id, requested_by, qty_requested, timer_expires_at)
        VALUES (?, ?, ?, datetime('now', '+48 hours'))
        RETURNING id
        "#,
        tile_id, auth.id, form.qty
    )
    .fetch_one(&state.db)
    .await?
    .id;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillRequested {
            refill_id,
            tile_id,
            item_number: tile.item_number,
        },
    );

    Ok(htmx_refresh())
}

// ── POST /refill/{id}/approve ─────────────────────────────────────────────────

async fn approve_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(refill_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    let req = sqlx::query!(
        r#"
        SELECT rr.id, rr.tile_id, rr.qty_requested, rr.status,
               t.branch_id, t.item_number, t.collection
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE rr.id = ? AND t.branch_id = ?
        "#,
        refill_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if req.status != "pending" {
        return Err(AppError::Conflict(format!(
            "Cannot approve a '{}' request",
            req.status
        )));
    }

    sqlx::query!(
        r#"UPDATE refill_requests
           SET status = 'approved', approved_by = ?, approved_at = datetime('now')
           WHERE id = ?"#,
        auth.id, refill_id
    )
    .execute(&state.db)
    .await?;

    // Salesforce — fail open
    let branch = sqlx::query!("SELECT name FROM branches WHERE id = ?", auth.branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .unwrap_or_default();

    state
        .salesforce
        .notify_refill(&RefillPayload {
            item_number: req.item_number,
            collection: req.collection,
            qty_requested: req.qty_requested,
            branch,
        })
        .await
        .unwrap_or_else(|e| tracing::error!("Salesforce notify_refill failed: {e}"));

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillStatusChange {
            refill_id,
            status: "approved".to_string(),
        },
    );

    Ok(htmx_refresh())
}

// ── POST /refill/{id}/reject ──────────────────────────────────────────────────

async fn reject_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(refill_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    let req = sqlx::query!(
        r#"
        SELECT rr.id, rr.status, t.branch_id
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE rr.id = ? AND t.branch_id = ?
        "#,
        refill_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if req.status != "pending" {
        return Err(AppError::Conflict(format!(
            "Cannot reject a '{}' request",
            req.status
        )));
    }

    sqlx::query!(
        r#"UPDATE refill_requests
           SET status = 'rejected', rejected_by = ?, rejected_at = datetime('now')
           WHERE id = ?"#,
        auth.id, refill_id
    )
    .execute(&state.db)
    .await?;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillStatusChange {
            refill_id,
            status: "rejected".to_string(),
        },
    );

    Ok(htmx_refresh())
}

// ── POST /refill/{id}/fulfill ─────────────────────────────────────────────────

async fn fulfill_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(refill_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    let req = sqlx::query!(
        r#"
        SELECT rr.id, rr.status, t.branch_id
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE rr.id = ? AND t.branch_id = ?
        "#,
        refill_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if req.status != "approved" {
        return Err(AppError::Conflict(format!(
            "Cannot fulfill a '{}' request",
            req.status
        )));
    }

    sqlx::query!(
        r#"UPDATE refill_requests
           SET status = 'fulfilled', fulfilled_by = ?, fulfilled_at = datetime('now')
           WHERE id = ?"#,
        auth.id, refill_id
    )
    .execute(&state.db)
    .await?;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillStatusChange {
            refill_id,
            status: "fulfilled".to_string(),
        },
    );

    Ok(htmx_refresh())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns HX-Refresh so HTMX reloads the page and reflects the updated refill state.
fn htmx_refresh() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("HX-Refresh", HeaderValue::from_static("true"));
    (StatusCode::OK, headers)
}
