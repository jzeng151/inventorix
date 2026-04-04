use axum::{
    extract::{Path, State},
    response::IntoResponse,
    routing::{get, post},
    Form, Router,
};
use serde::{Deserialize, Serialize};
use tera::Context;

use crate::{
    auth::extractor::{AuthUser, Role},
    ws::manager::WsEvent,
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/scan", get(scan_page))
        .route("/scan/print-qr", get(print_qr))
        .route("/tiles/by-qr/{item_number}", get(tile_by_qr))
        .route("/scan/restock-confirm", post(restock_confirm))
        .route("/scan/integrity-check", post(integrity_check))
}

// ── GET /scan ─────────────────────────────────────────────────────────────────

async fn scan_page(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }
    let branch = sqlx::query!("SELECT name FROM branches WHERE id = ?", auth.branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)?;

    let mut ctx = Context::new();
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch);
    state.render("scan/page.html", &ctx)
}

// ── GET /scan/print-qr ──────────────────────────���────────────────────────────

#[derive(Serialize)]
struct QrEntry {
    item_number: String,
    svg: String,
    collection: Option<String>,
    new_bin: Option<String>,
}

async fn print_qr(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    let tiles = sqlx::query!(
        "SELECT item_number, collection, new_bin FROM tiles WHERE branch_id = ? ORDER BY item_number",
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    let entries: Vec<QrEntry> = tiles
        .into_iter()
        .map(|t| {
            let svg = qr_svg(&t.item_number);
            QrEntry {
                item_number: t.item_number,
                svg,
                collection: t.collection,
                new_bin: t.new_bin,
            }
        })
        .collect();

    let branch = sqlx::query!("SELECT name FROM branches WHERE id = ?", auth.branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)?;

    let mut ctx = Context::new();
    ctx.insert("entries", &entries);
    ctx.insert("branch_name", &branch);
    state.render("scan/print-qr.html", &ctx)
}

// ── GET /tiles/by-qr/{item_number} ───────────────────────────────────────────

#[derive(Serialize)]
struct TileQrInfo {
    id: i64,
    item_number: String,
    qty: i64,
    health: String,
    collection: Option<String>,
    new_bin: Option<String>,
    refill_status: Option<String>,
    refill_id: Option<i64>,
    refill_qty: Option<i64>,
}

async fn tile_by_qr(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(item_number): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    let tile = sqlx::query!(
        "SELECT id, qty, low_inventory_threshold, collection, new_bin FROM tiles WHERE item_number = ? AND branch_id = ?",
        item_number, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let health = if tile.qty == 0 {
        "critical"
    } else if tile.qty <= tile.low_inventory_threshold {
        "low"
    } else {
        "healthy"
    };

    let refill = sqlx::query!(
        "SELECT id, status, qty_requested FROM refill_requests WHERE tile_id = ? AND status IN ('pending', 'approved') ORDER BY requested_at DESC LIMIT 1",
        tile.id
    )
    .fetch_optional(&state.db)
    .await?;

    let info = TileQrInfo {
        id: tile.id,
        item_number: item_number.clone(),
        qty: tile.qty,
        health: health.to_string(),
        collection: tile.collection,
        new_bin: tile.new_bin,
        refill_status: refill.as_ref().map(|r| r.status.clone()),
        refill_id: refill.as_ref().map(|r| r.id),
        refill_qty: refill.as_ref().map(|r| r.qty_requested),
    };

    Ok(axum::Json(info))
}

// ── POST /scan/restock-confirm ────────────────────────────────────────────────

#[derive(Deserialize)]
struct RestockConfirmForm {
    item_number: String,
}

#[derive(Serialize)]
struct RestockConfirmResult {
    ok: bool,
    item_number: String,
    new_qty: i64,
}

async fn restock_confirm(
    State(state): State<AppState>,
    auth: AuthUser,
    Form(form): Form<RestockConfirmForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    let item_number = form.item_number.trim().to_string();

    let tile = sqlx::query!(
        "SELECT id, qty FROM tiles WHERE item_number = ? AND branch_id = ?",
        item_number, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let refill = sqlx::query!(
        "SELECT id, qty_requested FROM refill_requests WHERE tile_id = ? AND status = 'approved' ORDER BY approved_at DESC LIMIT 1",
        tile.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::Conflict("No approved restock pending for this item".into()))?;

    // Fulfill the refill
    sqlx::query!(
        "UPDATE refill_requests SET status = 'fulfilled', fulfilled_by = ?, fulfilled_at = datetime('now') WHERE id = ?",
        auth.id, refill.id
    )
    .execute(&state.db)
    .await?;

    sqlx::query!(
        "UPDATE tiles SET qty = qty + ?, updated_at = datetime('now') WHERE id = ?",
        refill.qty_requested, tile.id
    )
    .execute(&state.db)
    .await?;

    let new_qty = sqlx::query!("SELECT qty FROM tiles WHERE id = ?", tile.id)
        .fetch_one(&state.db)
        .await?
        .qty;

    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::InventoryUpdate { tile_id: tile.id, new_qty },
    );
    state.ws_manager.broadcast(
        auth.branch_id,
        WsEvent::RefillStatusChange { refill_id: refill.id, status: "fulfilled".to_string() },
    );

    // Audit record
    sqlx::query!(
        r#"INSERT INTO scan_audits (tile_id, branch_id, scanned_by, scanned_by_name, mode, refill_id)
           VALUES (?, ?, ?, ?, 'restock_confirm', ?)"#,
        tile.id, auth.branch_id, auth.id, auth.name, refill.id
    )
    .execute(&state.db)
    .await?;

    Ok(axum::Json(RestockConfirmResult {
        ok: true,
        item_number,
        new_qty,
    }))
}

// ── POST /scan/integrity-check ────────────────────────────────────────────────

#[derive(Deserialize)]
struct IntegrityCheckForm {
    item_number: String,
    qty: i64,
}

#[derive(Serialize)]
struct IntegrityCheckResult {
    ok: bool,
    corrected: bool,
    old_qty: i64,
    new_qty: i64,
}

async fn integrity_check(
    State(state): State<AppState>,
    auth: AuthUser,
    Form(form): Form<IntegrityCheckForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    let item_number = form.item_number.trim().to_string();

    if form.qty < 0 || form.qty > 99_999 {
        return Err(AppError::ValidationError("Qty must be between 0 and 99,999".into()));
    }

    let tile = sqlx::query!(
        "SELECT id, qty FROM tiles WHERE item_number = ? AND branch_id = ?",
        item_number, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let old_qty = tile.qty;
    let new_qty = form.qty;
    let corrected = old_qty != new_qty;

    if corrected {
        sqlx::query!(
            "UPDATE tiles SET qty = ?, updated_at = datetime('now') WHERE id = ?",
            new_qty, tile.id
        )
        .execute(&state.db)
        .await?;

        state.ws_manager.broadcast(
            auth.branch_id,
            WsEvent::InventoryUpdate { tile_id: tile.id, new_qty },
        );
    }

    let corrected_i64 = corrected as i64;
    sqlx::query!(
        r#"INSERT INTO scan_audits (tile_id, branch_id, scanned_by, scanned_by_name, mode,
                                    qty_before, qty_after, corrected)
           VALUES (?, ?, ?, ?, 'integrity_check', ?, ?, ?)"#,
        tile.id, auth.branch_id, auth.id, auth.name,
        old_qty, new_qty, corrected_i64
    )
    .execute(&state.db)
    .await?;

    Ok(axum::Json(IntegrityCheckResult {
        ok: true,
        corrected,
        old_qty,
        new_qty,
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Generate a QR code SVG string encoding the given data.
fn qr_svg(data: &str) -> String {
    use qrcode::render::svg;
    use qrcode::QrCode;
    QrCode::new(data.as_bytes())
        .map(|code| {
            code.render::<svg::Color>()
                .min_dimensions(150, 150)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_else(|_| String::from("<svg/>"))
}
