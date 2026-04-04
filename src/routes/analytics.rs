use axum::{
    extract::{Query, State},
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tera::Context;

use crate::{
    auth::extractor::{AuthUser, Role},
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/analytics", get(analytics_page))
}

#[derive(Deserialize)]
struct AnalyticsQuery {
    date_from: Option<String>,
    date_to: Option<String>,
}

#[derive(Serialize, Clone)]
struct TileAnalytics {
    item_number: String,
    collection: String,
    gts_description: String,
    current_qty: i64,
    give_out_count: i64,
    restock_count: i64,
    pending_restock: bool, // has an approved-but-not-yet-fulfilled restock request
}

async fn analytics_page(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<AnalyticsQuery>,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin {
        return Err(AppError::Forbidden);
    }

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let date_from = params.date_from.filter(|s| !s.is_empty()).unwrap_or_else(|| today.clone());
    let date_to   = params.date_to.filter(|s| !s.is_empty()).unwrap_or_else(|| today.clone());

    // ── Give-outs per tile ────────────────────────────────────────────────────
    let give_out_rows = sqlx::query!(
        r#"
        SELECT cg.item_number,
               t.collection, t.gts_description, t.qty,
               COUNT(*) AS "give_out_count!: i64"
        FROM chat_give_outs cg
        LEFT JOIN tiles t ON t.item_number = cg.item_number AND t.branch_id = cg.branch_id
        WHERE cg.branch_id = ? AND date(cg.handled_at) BETWEEN ? AND ?
        GROUP BY cg.item_number
        "#,
        auth.branch_id, date_from, date_to
    )
    .fetch_all(&state.db)
    .await?;

    // ── Restock requests per tile ─────────────────────────────────────────────
    let restock_rows = sqlx::query!(
        r#"
        SELECT t.item_number, t.collection, t.gts_description, t.qty,
               COUNT(*) AS "restock_count!: i64"
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE t.branch_id = ? AND date(rr.requested_at) BETWEEN ? AND ?
        GROUP BY t.item_number
        "#,
        auth.branch_id, date_from, date_to
    )
    .fetch_all(&state.db)
    .await?;

    // ── Pending restocks (approved, not yet fulfilled) — not date-filtered ───
    let pending_rows = sqlx::query!(
        r#"
        SELECT t.item_number
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE t.branch_id = ? AND rr.status = 'approved'
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    let pending_items: std::collections::HashSet<String> =
        pending_rows.iter().map(|r| r.item_number.clone()).collect();

    let pending_restocks_count = pending_items.len() as i64;

    // ── Merge by item_number ──────────────────────────────────────────────────
    let mut map: HashMap<String, TileAnalytics> = HashMap::new();

    for r in &restock_rows {
        map.insert(r.item_number.clone(), TileAnalytics {
            item_number: r.item_number.clone(),
            collection: r.collection.clone().unwrap_or_default(),
            gts_description: r.gts_description.clone().unwrap_or_default(),
            current_qty: r.qty,
            give_out_count: 0,
            restock_count: r.restock_count,
            pending_restock: pending_items.contains(&r.item_number),
        });
    }

    for r in &give_out_rows {
        let pending = pending_items.contains(&r.item_number);
        let entry = map.entry(r.item_number.clone()).or_insert_with(|| TileAnalytics {
            item_number: r.item_number.clone(),
            collection: r.collection.clone().unwrap_or_default(),
            gts_description: r.gts_description.clone().unwrap_or_default(),
            current_qty: r.qty,
            give_out_count: 0,
            restock_count: 0,
            pending_restock: pending,
        });
        entry.give_out_count = r.give_out_count;
    }

    let mut entries: Vec<TileAnalytics> = map.into_values().collect();
    entries.sort_by(|a, b| {
        (b.give_out_count + b.restock_count).cmp(&(a.give_out_count + a.restock_count))
    });

    let total_give_outs: i64 = entries.iter().map(|e| e.give_out_count).sum();
    let total_restocks: i64  = entries.iter().map(|e| e.restock_count).sum();

    // ── Restock quantity aggregates (approved restocks in date range) ──────────
    let restock_agg = sqlx::query!(
        r#"
        SELECT
            COUNT(*)       AS "times_restocked!: i64",
            COALESCE(SUM(rr.qty_requested), 0) AS "total_qty_restocked!: i64",
            COALESCE(AVG(rr.qty_requested), 0) AS "avg_restock_amt!: f64"
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE t.branch_id = ?
          AND rr.status IN ('approved', 'fulfilled')
          AND date(rr.approved_at) BETWEEN ? AND ?
        "#,
        auth.branch_id, date_from, date_to
    )
    .fetch_one(&state.db)
    .await?;

    let times_restocked     = restock_agg.times_restocked;
    let total_qty_restocked = restock_agg.total_qty_restocked;
    let avg_restock_amt     = (restock_agg.avg_restock_amt * 10.0).round() / 10.0; // 1 decimal

    // ── Scan audit counts ─────────────────────────────────────────────────────
    let scan_agg = sqlx::query!(
        r#"
        SELECT
            COUNT(*) AS "total_scans!: i64",
            SUM(CASE WHEN sa.corrected = 1 THEN 1 ELSE 0 END) AS "total_corrections!: i64"
        FROM scan_audits sa
        JOIN tiles t ON sa.tile_id = t.id
        WHERE t.branch_id = ? AND date(sa.scanned_at) BETWEEN ? AND ?
        "#,
        auth.branch_id, date_from, date_to
    )
    .fetch_one(&state.db)
    .await?;

    let total_scans       = scan_agg.total_scans;
    let total_corrections = scan_agg.total_corrections;

    // ── Rejection count ───────────────────────────────────────────────────────
    let total_rejections = sqlx::query!(
        r#"
        SELECT COUNT(*) AS "count!: i64"
        FROM refill_requests rr
        JOIN tiles t ON rr.tile_id = t.id
        WHERE t.branch_id = ? AND rr.status = 'rejected'
          AND date(rr.rejected_at) BETWEEN ? AND ?
        "#,
        auth.branch_id, date_from, date_to
    )
    .fetch_one(&state.db)
    .await?
    .count;

    let entries_json = serde_json::to_string(&entries)
        .unwrap_or_else(|_| "[]".into());

    let branch = sqlx::query!("SELECT name FROM branches WHERE id = ?", auth.branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)?;

    let mut ctx = Context::new();
    ctx.insert("entries", &entries);
    ctx.insert("entries_json", &entries_json);
    ctx.insert("total_give_outs", &total_give_outs);
    ctx.insert("total_restocks", &total_restocks);
    ctx.insert("times_restocked", &times_restocked);
    ctx.insert("total_qty_restocked", &total_qty_restocked);
    ctx.insert("avg_restock_amt", &avg_restock_amt);
    ctx.insert("total_rejections", &total_rejections);
    ctx.insert("pending_restocks_count", &pending_restocks_count);
    ctx.insert("total_scans", &total_scans);
    ctx.insert("total_corrections", &total_corrections);
    ctx.insert("date_from", &date_from);
    ctx.insert("date_to", &date_to);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch);
    state.render("analytics/page.html", &ctx)
}
