use axum::{
    extract::{Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Redirect},
    routing::{get, post, put},
    Form, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::Context;

use crate::{
    auth::extractor::{AuthUser, Role},
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(inventory_table))
        .route("/tiles/{id}", get(tile_detail))
        .route("/tiles/{id}", put(update_tile))
        .route("/tiles/{id}/card", get(tile_card))
        .route("/tiles/{id}/note", post(edit_note))
}

// ── DB row types (returned by sqlx) ──────────────────────────────────────────

#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)]
struct TileRow {
    id: i64,
    branch_id: i64,
    item_number: String,
    collection: Option<String>,
    gts_description: Option<String>,
    new_bin: Option<String>,
    qty: i64,
    overflow_rack: i64,
    order_number: Option<String>,
    notes: Option<String>,
    sample_coordinator_id: Option<i64>,
    sales_rep_id: Option<i64>,
    low_inventory_threshold: i64,
    created_at: String,
    updated_at: String,
    coordinator_name: Option<String>,
    sales_rep_name: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct ActiveRefill {
    tile_id: i64,
    id: i64,
    status: String,
    timer_expires_at: String,
    qty_requested: i64,
    approved_by_name: Option<String>,
    approved_at: Option<String>,
    requested_at: String,
}

// ── Template view models (serialized to Tera) ─────────────────────────────────

#[derive(Debug, Serialize)]
struct TileView {
    id: i64,
    item_number: String,
    collection: Option<String>,
    gts_description: Option<String>,
    new_bin: Option<String>,
    qty: i64,
    overflow_rack: bool,
    order_number: Option<String>,
    notes: Option<String>,
    low_inventory_threshold: i64,
    coordinator_name: Option<String>,
    sales_rep_name: Option<String>,
    health: String,
    refill_status: Option<String>,
    refill_countdown: Option<String>,
    refill_id: Option<i64>,
    refill_qty: Option<i64>,
    refill_approved_by: Option<String>,
    refill_approved_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct InventoryStats {
    critical: usize,
    low: usize,
    healthy: usize,
    total: usize,
}

// ── GET / — inventory table ───────────────────────────────────────────────────

pub async fn inventory_table(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    // Single query: tiles + joined coordinator/rep names, sorted by health
    let rows = sqlx::query_as!(
        TileRow,
        r#"
        SELECT
            t.id, t.branch_id, t.item_number, t.collection, t.gts_description,
            t.new_bin, t.qty, t.overflow_rack, t.order_number, t.notes,
            t.sample_coordinator_id, t.sales_rep_id, t.low_inventory_threshold,
            t.created_at, t.updated_at,
            c.name AS coordinator_name,
            r.name AS sales_rep_name
        FROM tiles t
        LEFT JOIN users c ON t.sample_coordinator_id = c.id
        LEFT JOIN users r ON t.sales_rep_id = r.id
        WHERE t.branch_id = ?
        ORDER BY
            CASE WHEN t.qty = 0 THEN 0
                 WHEN t.qty <= t.low_inventory_threshold THEN 1
                 ELSE 2 END,
            t.item_number
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // One query for all active refill requests — not N+1
    let refills = sqlx::query_as!(
        ActiveRefill,
        r#"
        SELECT rr.tile_id, rr.id, rr.status, rr.timer_expires_at, rr.qty_requested,
               u.name AS approved_by_name, rr.approved_at, rr.requested_at
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        LEFT JOIN users u ON rr.approved_by = u.id
        WHERE t.branch_id = ? AND rr.status IN ('pending', 'approved')
        ORDER BY rr.requested_at DESC
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // Map tile_id → most recent active refill
    let mut refill_map: HashMap<i64, &ActiveRefill> = HashMap::new();
    for r in &refills {
        refill_map.entry(r.tile_id).or_insert(r);
    }

    let now = Utc::now();
    let mut stats = InventoryStats { critical: 0, low: 0, healthy: 0, total: rows.len() };

    let tiles: Vec<TileView> = rows
        .iter()
        .map(|t| {
            let health = tile_health(t.qty, t.low_inventory_threshold);
            match health.as_str() {
                "critical" => stats.critical += 1,
                "low" => stats.low += 1,
                _ => stats.healthy += 1,
            }
            let refill = refill_map.get(&t.id).copied();
            tile_view_from(t, refill, &health, now)
        })
        .collect();

    let branch_name = branch_name(&state, auth.branch_id).await?;

    let mut ctx = Context::new();
    ctx.insert("tiles", &tiles);
    ctx.insert("stats", &stats);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch_name);

    state.render("inventory/table.html", &ctx)
}

// ── GET /tiles/:id — tile detail ──────────────────────────────────────────────

pub async fn tile_detail(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tile_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let tile = sqlx::query_as!(
        TileRow,
        r#"
        SELECT
            t.id, t.branch_id, t.item_number, t.collection, t.gts_description,
            t.new_bin, t.qty, t.overflow_rack, t.order_number, t.notes,
            t.sample_coordinator_id, t.sales_rep_id, t.low_inventory_threshold,
            t.created_at, t.updated_at,
            c.name AS coordinator_name,
            r.name AS sales_rep_name
        FROM tiles t
        LEFT JOIN users c ON t.sample_coordinator_id = c.id
        LEFT JOIN users r ON t.sales_rep_id = r.id
        WHERE t.id = ? AND t.branch_id = ?
        "#,
        tile_id,
        auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let refill = sqlx::query_as!(
        ActiveRefill,
        r#"
        SELECT rr.tile_id, rr.id, rr.status, rr.timer_expires_at, rr.qty_requested,
               u.name AS approved_by_name, rr.approved_at, rr.requested_at
        FROM refill_requests rr
        LEFT JOIN users u ON rr.approved_by = u.id
        WHERE rr.tile_id = ? AND rr.status IN ('pending', 'approved')
        ORDER BY rr.requested_at DESC LIMIT 1
        "#,
        tile_id
    )
    .fetch_optional(&state.db)
    .await?;

    let now = Utc::now();
    let health = tile_health(tile.qty, tile.low_inventory_threshold);
    let tile_view = tile_view_from(&tile, refill.as_ref(), &health, now);
    let branch_name = branch_name(&state, auth.branch_id).await?;

    let mut ctx = Context::new();
    ctx.insert("tile", &tile_view);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch_name);

    state.render("tiles/detail.html", &ctx)
}

// ── GET /tiles/:id/card — mini card partial for dialog ───────────────────────

async fn tile_card(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tile_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let tile = sqlx::query_as!(
        TileRow,
        r#"
        SELECT
            t.id, t.branch_id, t.item_number, t.collection, t.gts_description,
            t.new_bin, t.qty, t.overflow_rack, t.order_number, t.notes,
            t.sample_coordinator_id, t.sales_rep_id, t.low_inventory_threshold,
            t.created_at, t.updated_at,
            c.name AS coordinator_name,
            r.name AS sales_rep_name
        FROM tiles t
        LEFT JOIN users c ON t.sample_coordinator_id = c.id
        LEFT JOIN users r ON t.sales_rep_id = r.id
        WHERE t.id = ? AND t.branch_id = ?
        "#,
        tile_id,
        auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let refill = sqlx::query_as!(
        ActiveRefill,
        r#"
        SELECT rr.tile_id, rr.id, rr.status, rr.timer_expires_at, rr.qty_requested,
               u.name AS approved_by_name, rr.approved_at, rr.requested_at
        FROM refill_requests rr
        LEFT JOIN users u ON rr.approved_by = u.id
        WHERE rr.tile_id = ? AND rr.status IN ('pending', 'approved')
        ORDER BY rr.requested_at DESC LIMIT 1
        "#,
        tile_id
    )
    .fetch_optional(&state.db)
    .await?;

    let now = Utc::now();
    let health = tile_health(tile.qty, tile.low_inventory_threshold);
    let tile_view = tile_view_from(&tile, refill.as_ref(), &health, now);

    let mut ctx = Context::new();
    ctx.insert("tile", &tile_view);
    ctx.insert("auth_user_role", auth.role.as_str());

    state.render("tiles/card.html", &ctx)
}

// ── PUT /tiles/:id — update qty / notes / assignments ────────────────────────

#[derive(Deserialize)]
pub struct UpdateTileForm {
    pub qty: Option<i64>,
    pub notes: Option<String>,
    pub sample_coordinator_id: Option<i64>,
    pub sales_rep_id: Option<i64>,
}

pub async fn update_tile(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tile_id): Path<i64>,
    Form(form): Form<UpdateTileForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    // Branch isolation — 404 rather than 403 to avoid leaking tile existence
    let tile = sqlx::query!(
        "SELECT id, qty FROM tiles WHERE id = ? AND branch_id = ?",
        tile_id,
        auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if let Some(new_qty) = form.qty {
        let old_qty = tile.qty;
        sqlx::query!(
            "UPDATE tiles SET qty = ?, updated_at = datetime('now') WHERE id = ?",
            new_qty,
            tile_id
        )
        .execute(&state.db)
        .await?;

        sqlx::query!(
            r#"INSERT INTO inventory_events (tile_id, event_type, old_qty, new_qty, user_id)
               VALUES (?, 'manual_edit', ?, ?, ?)"#,
            tile_id,
            old_qty,
            new_qty,
            auth.id
        )
        .execute(&state.db)
        .await?;

        // Broadcast to all connections in the branch (Lane C delivers this)
        state.ws_manager.broadcast(
            auth.branch_id,
            crate::ws::manager::WsEvent::InventoryUpdate { tile_id, new_qty },
        );
    }

    if let Some(ref notes) = form.notes {
        if notes.len() > 2_000 {
            return Err(AppError::ValidationError(
                "Notes must be 2,000 characters or fewer".into(),
            ));
        }
    }

    // Validate assigned users belong to the same branch (prevent cross-branch assignment)
    if let Some(cid) = form.sample_coordinator_id {
        sqlx::query!(
            "SELECT id FROM users WHERE id = ? AND branch_id = ? AND is_active = 1",
            cid, auth.branch_id
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::ValidationError(
            "Coordinator not found in this branch".into(),
        ))?;
    }
    if let Some(rid) = form.sales_rep_id {
        sqlx::query!(
            "SELECT id FROM users WHERE id = ? AND branch_id = ? AND is_active = 1",
            rid, auth.branch_id
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| AppError::ValidationError(
            "Sales rep not found in this branch".into(),
        ))?;
    }

    if form.notes.is_some() || form.sample_coordinator_id.is_some() || form.sales_rep_id.is_some()
    {
        sqlx::query!(
            r#"UPDATE tiles
               SET notes                  = COALESCE(?, notes),
                   sample_coordinator_id  = COALESCE(?, sample_coordinator_id),
                   sales_rep_id           = COALESCE(?, sales_rep_id),
                   updated_at             = datetime('now')
               WHERE id = ?"#,
            form.notes,
            form.sample_coordinator_id,
            form.sales_rep_id,
            tile_id
        )
        .execute(&state.db)
        .await?;
    }

    Ok(Redirect::to(&format!("/tiles/{tile_id}")))
}

// ── POST /tiles/:id/note — edit note and log to history ──────────────────────

#[derive(Deserialize)]
struct NoteForm {
    note: String,
}

async fn edit_note(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(tile_id): Path<i64>,
    Form(form): Form<NoteForm>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }
    if form.note.len() > 2_000 {
        return Err(AppError::ValidationError(
            "Note must be 2,000 characters or fewer".into(),
        ));
    }

    let tile = sqlx::query!(
        "SELECT id, notes FROM tiles WHERE id = ? AND branch_id = ?",
        tile_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let old_note = tile.notes;
    let new_note: Option<String> = if form.note.trim().is_empty() {
        None
    } else {
        Some(form.note.clone())
    };

    sqlx::query!(
        "UPDATE tiles SET notes = ?, updated_at = datetime('now') WHERE id = ?",
        new_note, tile_id
    )
    .execute(&state.db)
    .await?;

    sqlx::query!(
        r#"INSERT INTO note_edits (tile_id, branch_id, old_note, new_note, edited_by, edited_by_name)
           VALUES (?, ?, ?, ?, ?, ?)"#,
        tile_id, auth.branch_id, old_note, new_note, auth.id, auth.name
    )
    .execute(&state.db)
    .await?;

    let mut headers = HeaderMap::new();
    headers.insert("HX-Refresh", HeaderValue::from_static("true"));
    Ok((StatusCode::OK, headers))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn tile_health(qty: i64, threshold: i64) -> String {
    if qty == 0 {
        "critical".to_string()
    } else if qty <= threshold {
        "low".to_string()
    } else {
        "healthy".to_string()
    }
}

/// Format a SQLite datetime string ("YYYY-MM-DD HH:MM:SS") as an elapsed duration.
fn fmt_elapsed(requested_at: &str, now: chrono::DateTime<Utc>) -> String {
    use chrono::NaiveDateTime;
    let Ok(naive) = NaiveDateTime::parse_from_str(requested_at, "%Y-%m-%d %H:%M:%S") else {
        return "—".to_string();
    };
    let elapsed = now.signed_duration_since(naive.and_utc());
    let total_minutes = elapsed.num_minutes().max(0);
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours == 0 {
        format!("{minutes}m")
    } else {
        format!("{hours}h {minutes}m")
    }
}

fn tile_view_from(
    t: &TileRow,
    refill: Option<&ActiveRefill>,
    health: &str,
    now: chrono::DateTime<Utc>,
) -> TileView {
    TileView {
        id: t.id,
        item_number: t.item_number.clone(),
        collection: t.collection.clone(),
        gts_description: t.gts_description.clone(),
        new_bin: t.new_bin.clone(),
        qty: t.qty,
        overflow_rack: t.overflow_rack != 0,
        order_number: t.order_number.clone(),
        notes: t.notes.clone(),
        low_inventory_threshold: t.low_inventory_threshold,
        coordinator_name: t.coordinator_name.clone(),
        sales_rep_name: t.sales_rep_name.clone(),
        health: health.to_string(),
        refill_status: refill.map(|r| r.status.clone()),
        refill_countdown: refill.and_then(|r| {
            (r.status == "pending").then(|| fmt_elapsed(&r.requested_at, now))
        }),
        refill_id: refill.map(|r| r.id),
        refill_qty: refill.map(|r| r.qty_requested),
        refill_approved_by: refill.and_then(|r| r.approved_by_name.clone()),
        refill_approved_at: refill.and_then(|r| r.approved_at.clone()),
    }
}

async fn branch_name(state: &AppState, branch_id: i64) -> Result<String, AppError> {
    Ok(sqlx::query_scalar!("SELECT name FROM branches WHERE id = ?", branch_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_else(|| "—".to_string()))
}
