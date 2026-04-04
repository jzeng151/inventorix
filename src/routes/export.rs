use axum::{
    body::Body,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Router,
};
use rust_xlsxwriter::{Format, Workbook};
use tera::Context;

use crate::{auth::extractor::AuthUser, AppError, AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/export", post(export_handler))
}

// ── POST /export ──────────────────────────────────────────────────────────────

async fn export_handler(
    axum::extract::State(state): axum::extract::State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Fetch all tiles for this branch with joined user names
    let rows = sqlx::query!(
        r#"
        SELECT
            t.item_number, t.collection, t.gts_description, t.new_bin,
            t.qty, t.overflow_rack, t.order_number, t.notes,
            t.updated_at,
            c.name AS coordinator_name,
            r.name  AS sales_rep_name,
            (SELECT status FROM refill_requests
             WHERE tile_id = t.id AND status IN ('pending','approved')
             ORDER BY requested_at DESC LIMIT 1) AS refill_status
        FROM tiles t
        LEFT JOIN users c ON t.sample_coordinator_id = c.id
        LEFT JOIN users r ON t.sales_rep_id          = r.id
        WHERE t.branch_id = ?
        ORDER BY t.item_number
        "#,
        auth.branch_id
    )
    .fetch_all(&state.db)
    .await?;

    // ── Build .xlsx ───────────────────────────────────────────────────────────
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();

    let header_fmt = Format::new().set_bold();
    let headers = [
        "ITEM NUMBER",
        "COLLECTION",
        "GTS DESCRIPTION",
        "NEW BIN",
        "QTY",
        "OVERFLOW RACK",
        "ORDER #",
        "NOTES",
        "COORDINATOR",
        "SALES REP",
        "REFILL STATUS",
        "LAST UPDATED",
    ];
    for (col, h) in headers.iter().enumerate() {
        ws.write_with_format(0, col as u16, *h, &header_fmt)
            .map_err(|e| AppError::Internal(format!("xlsx header: {e}")))?;
    }

    for (i, row) in rows.iter().enumerate() {
        let r = (i + 1) as u32;
        ws.write(r, 0, &row.item_number).ok();
        ws.write(r, 1, row.collection.as_deref().unwrap_or("")).ok();
        ws.write(r, 2, row.gts_description.as_deref().unwrap_or("")).ok();
        ws.write(r, 3, row.new_bin.as_deref().unwrap_or("")).ok();
        ws.write(r, 4, row.qty).ok();
        ws.write(r, 5, if row.overflow_rack != 0 { "Y" } else { "" }).ok();
        ws.write(r, 6, row.order_number.as_deref().unwrap_or("")).ok();
        ws.write(r, 7, row.notes.as_deref().unwrap_or("")).ok();
        ws.write(r, 8, &row.coordinator_name).ok();
        ws.write(r, 9, &row.sales_rep_name).ok();
        ws.write(r, 10, &row.refill_status).ok();
        ws.write(r, 11, &row.updated_at).ok();
    }

    let xlsx_bytes = wb
        .save_to_buffer()
        .map_err(|e| AppError::Internal(format!("xlsx save: {e}")))?;

    // ── Build HTML digest ─────────────────────────────────────────────────────
    #[derive(serde::Serialize)]
    struct DigestRow {
        item_number: String,
        collection: String,
        new_bin: String,
        qty: i64,
        refill_status: String,
        updated_at: String,
    }

    let recently_updated: Vec<DigestRow> = rows
        .iter()
        .filter(|r| r.updated_at.starts_with(&today))
        .map(|r| DigestRow {
            item_number: r.item_number.clone(),
            collection: r.collection.clone().unwrap_or_default(),
            new_bin: r.new_bin.clone().unwrap_or_default(),
            qty: r.qty,
            refill_status: r.refill_status.clone(),
            updated_at: r.updated_at.clone(),
        })
        .collect();

    let critical: Vec<DigestRow> = rows
        .iter()
        .filter(|r| r.qty == 0)
        .map(|r| DigestRow {
            item_number: r.item_number.clone(),
            collection: r.collection.clone().unwrap_or_default(),
            new_bin: r.new_bin.clone().unwrap_or_default(),
            qty: r.qty,
            refill_status: r.refill_status.clone(),
            updated_at: r.updated_at.clone(),
        })
        .collect();

    let mut ctx = Context::new();
    ctx.insert("today", &today);
    ctx.insert("total", &rows.len());
    ctx.insert("recently_updated", &recently_updated);
    ctx.insert("critical", &critical);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("branch_name", &branch_name(&state, auth.branch_id).await?);

    let digest_html = state
        .tera
        .render("export/digest.html", &ctx)
        .map_err(AppError::Template)?;

    // ── Return .xlsx as download; digest is stored separately ─────────────────
    // Save digest to export dir alongside xlsx
    let export_dir = format!("./exports/{}", auth.branch_id);
    tokio::fs::create_dir_all(&export_dir)
        .await
        .map_err(|e| AppError::Internal(format!("export dir: {e}")))?;

    let digest_path = format!("{export_dir}/digest-{today}.html");
    tokio::fs::write(&digest_path, digest_html.as_bytes())
        .await
        .map_err(|e| AppError::Internal(format!("digest write: {e}")))?;

    let xlsx_filename = format!("inventory-{today}.xlsx");
    let response = Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{xlsx_filename}\""),
        )
        .header("X-Digest-Path", &digest_path)
        .body(Body::from(xlsx_bytes))
        .map_err(|e| AppError::Internal(format!("response build: {e}")))?;

    Ok(response)
}

async fn branch_name(state: &AppState, branch_id: i64) -> Result<String, AppError> {
    sqlx::query!("SELECT name FROM branches WHERE id = ?", branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)
}
