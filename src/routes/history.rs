use axum::{
    extract::State,
    response::IntoResponse,
    routing::get,
    Router,
};
use serde::Serialize;
use tera::Context;

use crate::{
    auth::extractor::AuthUser,
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/history", get(history_page))
}

#[derive(Serialize)]
struct HistoryEntry {
    ts: String,
    date_str: String,
    time_str: String,
    item_number: String,
    action: String,
    action_slug: String, // css class suffix: requested | approved | fulfilled | rejected | giveout
    person: String,
    qty: Option<i64>,            // qty for restock events; None for give-outs
    processing_time: Option<String>,    // only set on "Restock Fulfilled"
    rejection_reason: Option<String>,  // only set on "Restock Rejected"
    pending_fulfillment: bool,         // true on Approved entries awaiting coordinator confirm
}

async fn history_page(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    // ── Refill request events ─────────────────────────────────────────────────
    let refill_rows = sqlx::query!(
        r#"
        SELECT
            t.item_number,
            rr.qty_requested,
            rr.requested_at,
            rr.approved_at,
            rr.fulfilled_at,
            rr.rejected_at,
            rr.rejection_reason,
            req.name  AS requested_by_name,
            ap.name   AS approved_by_name,
            ful.name  AS fulfilled_by_name,
            rej.name  AS rejected_by_name
        FROM refill_requests rr
        JOIN  tiles t   ON rr.tile_id       = t.id
        JOIN  users req ON rr.requested_by  = req.id
        LEFT JOIN users ap  ON rr.approved_by  = ap.id
        LEFT JOIN users ful ON rr.fulfilled_by = ful.id
        LEFT JOIN users rej ON rr.rejected_by  = rej.id
        WHERE t.branch_id = ?
        ORDER BY rr.requested_at DESC
        LIMIT 500
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // ── Note edit events ──────────────────────────────────────────────────────
    let note_rows = sqlx::query!(
        r#"
        SELECT ne.edited_at, ne.edited_by_name, t.item_number
        FROM note_edits ne
        JOIN tiles t ON ne.tile_id = t.id
        WHERE ne.branch_id = ?
        ORDER BY ne.edited_at DESC
        LIMIT 500
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // ── Give-out events ───────────────────────────────────────────────────────
    let give_out_rows = sqlx::query!(
        r#"
        SELECT item_number, handled_by_name, handled_at
        FROM chat_give_outs
        WHERE branch_id = ?
        ORDER BY handled_at DESC
        LIMIT 500
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // ── Merge into unified list ───────────────────────────────────────────────
    let mut entries: Vec<HistoryEntry> = Vec::new();

    for r in &refill_rows {
        let (time_str, date_str) = fmt_ts(&r.requested_at);
        entries.push(HistoryEntry {
            ts: r.requested_at.clone(),
            date_str, time_str,
            item_number: r.item_number.clone(),
            action: "Restock Requested".into(),
            action_slug: "requested".into(),
            person: r.requested_by_name.clone(),
            qty: Some(r.qty_requested),
            processing_time: None,
            rejection_reason: None,
            pending_fulfillment: false,
        });

        if let Some(ts) = &r.approved_at {
            let (time_str, date_str) = fmt_ts(ts);
            // Pending fulfillment = approved but not yet fulfilled or rejected
            let pending = r.fulfilled_at.is_none() && r.rejected_at.is_none();
            entries.push(HistoryEntry {
                ts: ts.clone(),
                date_str, time_str,
                item_number: r.item_number.clone(),
                action: "Restock Approved".into(),
                action_slug: "approved".into(),
                person: r.approved_by_name.clone().unwrap_or_else(|| "Unknown".into()),
                qty: Some(r.qty_requested),
                processing_time: None,
                rejection_reason: None,
                pending_fulfillment: pending,
            });
        }

        if let Some(ts) = &r.fulfilled_at {
            let (time_str, date_str) = fmt_ts(ts);
            entries.push(HistoryEntry {
                ts: ts.clone(),
                date_str, time_str,
                item_number: r.item_number.clone(),
                action: "Restock Fulfilled".into(),
                action_slug: "fulfilled".into(),
                person: r.fulfilled_by_name.clone().unwrap_or_else(|| "Unknown".into()),
                qty: Some(r.qty_requested),
                processing_time: fmt_duration(&r.requested_at, ts),
                rejection_reason: None,
                pending_fulfillment: false,
            });
        }

        if let Some(ts) = &r.rejected_at {
            let (time_str, date_str) = fmt_ts(ts);
            entries.push(HistoryEntry {
                ts: ts.clone(),
                date_str, time_str,
                item_number: r.item_number.clone(),
                action: "Restock Rejected".into(),
                action_slug: "rejected".into(),
                person: r.rejected_by_name.clone().unwrap_or_else(|| "Unknown".into()),
                qty: None,
                processing_time: None,
                rejection_reason: r.rejection_reason.clone(),
                pending_fulfillment: false,
            });
        }
    }

    for r in &give_out_rows {
        let (time_str, date_str) = fmt_ts(&r.handled_at);
        entries.push(HistoryEntry {
            ts: r.handled_at.clone(),
            date_str, time_str,
            item_number: r.item_number.clone(),
            action: "Give Out".into(),
            action_slug: "giveout".into(),
            person: r.handled_by_name.clone(),
            qty: None,
            processing_time: None,
            rejection_reason: None,
            pending_fulfillment: false,
        });
    }

    for r in &note_rows {
        let (time_str, date_str) = fmt_ts(&r.edited_at);
        entries.push(HistoryEntry {
            ts: r.edited_at.clone(),
            date_str, time_str,
            item_number: r.item_number.clone(),
            action: "Note Edited".into(),
            action_slug: "note-edited".into(),
            person: r.edited_by_name.clone(),
            qty: None,
            processing_time: None,
            rejection_reason: None,
            pending_fulfillment: false,
        });
    }

    // Newest first
    entries.sort_by(|a, b| b.ts.cmp(&a.ts));

    let branch = sqlx::query!("SELECT name FROM branches WHERE id = ?", auth.branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)?;

    let mut ctx = Context::new();
    ctx.insert("entries", &entries);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch);
    state.render("history/page.html", &ctx)
}

/// Parse a SQLite datetime string ("YYYY-MM-DD HH:MM:SS") or RFC 3339,
/// return (time_str, date_str) formatted for display.
fn fmt_ts(ts: &str) -> (String, String) {
    use chrono::{Datelike, Timelike};
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];

    // Try SQLite datetime format first
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S") {
        let h24 = dt.hour();
        let h12 = match h24 % 12 { 0 => 12, h => h };
        let ampm = if h24 >= 12 { "PM" } else { "AM" };
        return (
            format!("{}:{:02} {}", h12, dt.minute(), ampm),
            format!("{} {}, {}", MONTHS[dt.month0() as usize], dt.day(), dt.year()),
        );
    }

    // Fall back to RFC 3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        let h24 = dt.hour();
        let h12 = match h24 % 12 { 0 => 12, h => h };
        let ampm = if h24 >= 12 { "PM" } else { "AM" };
        return (
            format!("{}:{:02} {}", h12, dt.minute(), ampm),
            format!("{} {}, {}", MONTHS[dt.month0() as usize], dt.day(), dt.year()),
        );
    }

    ("—".into(), "—".into())
}

/// Compute elapsed duration between two SQLite datetime strings.
/// Returns e.g. "2h 34m" or "45m", or None if either string fails to parse.
fn fmt_duration(start: &str, end: &str) -> Option<String> {
    let s = chrono::NaiveDateTime::parse_from_str(start, "%Y-%m-%d %H:%M:%S").ok()?;
    let e = chrono::NaiveDateTime::parse_from_str(end,   "%Y-%m-%d %H:%M:%S").ok()?;
    let total_minutes = e.signed_duration_since(s).num_minutes().max(0);
    let hours   = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours == 0 {
        Some(format!("Processed in {minutes}m"))
    } else {
        Some(format!("Processed in {hours}h {minutes}m"))
    }
}
