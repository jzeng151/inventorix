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
            rr.requested_at,
            rr.approved_at,
            rr.fulfilled_at,
            rr.rejected_at,
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
        });

        if let Some(ts) = &r.approved_at {
            let (time_str, date_str) = fmt_ts(ts);
            entries.push(HistoryEntry {
                ts: ts.clone(),
                date_str, time_str,
                item_number: r.item_number.clone(),
                action: "Restock Approved".into(),
                action_slug: "approved".into(),
                person: r.approved_by_name.clone().unwrap_or_else(|| "Unknown".into()),
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
