use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Form, Router,
};
use serde::Deserialize;
use serde_json;

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
        .route("/refills/approved", get(approved_refills_json))
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

#[derive(Deserialize)]
struct ApproveForm {
    qty: i64,
}

async fn approve_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(refill_id): Path<i64>,
    Form(form): Form<ApproveForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }
    if form.qty <= 0 {
        return Err(AppError::ValidationError(
            "Approved quantity must be greater than 0".into(),
        ));
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
           SET status = 'approved', qty_requested = ?,
               approved_by = ?, approved_at = datetime('now')
           WHERE id = ?"#,
        form.qty, auth.id, refill_id
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
            qty_requested: form.qty,
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

#[derive(Deserialize)]
struct RejectForm {
    reason: String,
}

async fn reject_refill(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(refill_id): Path<i64>,
    Form(form): Form<RejectForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    // Sanitize: trim whitespace, enforce non-empty and max length
    let reason = form.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::ValidationError("Rejection reason is required".into()));
    }
    if reason.len() > 500 {
        return Err(AppError::ValidationError(
            "Rejection reason must be 500 characters or fewer".into(),
        ));
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
           SET status = 'rejected', rejected_by = ?, rejected_at = datetime('now'),
               rejection_reason = ?
           WHERE id = ?"#,
        auth.id, reason, refill_id
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
        SELECT rr.id, rr.status, rr.qty_requested, rr.tile_id, t.branch_id
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

    sqlx::query!(
        "UPDATE tiles SET qty = qty + ?, updated_at = datetime('now') WHERE id = ?",
        req.qty_requested, req.tile_id
    )
    .execute(&state.db)
    .await?;

    let new_qty = sqlx::query!("SELECT qty FROM tiles WHERE id = ?", req.tile_id)
        .fetch_one(&state.db)
        .await?
        .qty;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::InventoryUpdate { tile_id: req.tile_id, new_qty },
    );

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillStatusChange {
            refill_id,
            status: "fulfilled".to_string(),
        },
    );

    Ok(htmx_refresh())
}

// ── GET /refills/approved — coordinator notification data ─────────────────────

async fn approved_refills_json(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    let rows = sqlx::query!(
        r#"
        SELECT rr.id, rr.tile_id, rr.qty_requested, t.item_number
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE t.branch_id = ? AND rr.status = 'approved'
        ORDER BY rr.approved_at DESC
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.id,
                "tile_id": r.tile_id,
                "item_number": r.item_number,
                "qty": r.qty_requested,
            })
        })
        .collect();

    Ok(axum::Json(items))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns HX-Refresh so HTMX reloads the page and reflects the updated refill state.
fn htmx_refresh() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert("HX-Refresh", HeaderValue::from_static("true"));
    (StatusCode::OK, headers)
}
