use axum::{
    extract::{Multipart, State},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use calamine::{open_workbook_from_rs, Reader, Xlsx};
use std::io::Cursor;
use tera::Context;

use crate::{
    auth::extractor::{AuthUser, Role},
    AppError, AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/import", get(import_page))
        .route("/import", post(import_upload))
}

// ── GET /import ───────────────────────────────────────────────────────────────

async fn import_page(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<impl IntoResponse, AppError> {
    if auth.role != Role::Admin && auth.role != Role::Coordinator {
        return Err(AppError::Forbidden);
    }
    let mut ctx = Context::new();
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch_name(&state, auth.branch_id).await?);
    state.render("import/page.html", &ctx)
}

// ── POST /import ──────────────────────────────────────────────────────────────

async fn import_upload(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    if auth.role == Role::SalesRep {
        return Err(AppError::Forbidden);
    }

    // Acquire import lock — 409 if another import is running
    {
        let mut lock = state.import_lock.lock().await;
        if *lock {
            return Err(AppError::Conflict("Import already in progress".into()));
        }
        *lock = true;
    }

    let result = run_import(&state, auth, multipart).await;
    *state.import_lock.lock().await = false;
    result
}

// ── Import logic ──────────────────────────────────────────────────────────────

#[derive(serde::Serialize)]
struct ImportResult {
    upserted: usize,
    skipped: usize,
    errors: Vec<String>,
}

async fn run_import(
    state: &AppState,
    auth: AuthUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut file_name = String::new();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::ValidationError(format!("Upload error: {e}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().unwrap_or("upload").to_string();
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| AppError::ValidationError(format!("Read error: {e}")))?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = file_bytes
        .ok_or_else(|| AppError::ValidationError("No file uploaded".into()))?;

    if file_name.to_lowercase().ends_with(".xls")
        && !file_name.to_lowercase().ends_with(".xlsx")
    {
        return Err(AppError::ValidationError(
            "Legacy .xls format not supported — please save as .xlsx first.".into(),
        ));
    }

    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> = open_workbook_from_rs(cursor)
        .map_err(|e| AppError::ValidationError(format!("Cannot open file: {e}")))?;

    let sheet_name = workbook
        .sheet_names()
        .first()
        .cloned()
        .ok_or_else(|| AppError::ValidationError("Workbook has no sheets".into()))?;

    let range = workbook
        .worksheet_range(&sheet_name)
        .map_err(|e| AppError::ValidationError(format!("Cannot read sheet: {e}")))?;

    let mut rows = range.rows();

    // Validate required columns by position (flexible uppercase match)
    let header = rows
        .next()
        .ok_or_else(|| AppError::ValidationError("Sheet is empty".into()))?;

    for (idx, name) in &[(1usize, "ITEM"), (5usize, "QTY")] {
        let cell = header
            .get(*idx)
            .map(|c| c.to_string().trim().to_uppercase())
            .unwrap_or_default();
        if !cell.contains(name) {
            return Err(AppError::ValidationError(format!(
                "Column {} expected to contain '{name}', got '{cell}'",
                idx + 1
            )));
        }
    }

    struct TileRecord {
        item_number: String,
        collection: Option<String>,
        gts_description: Option<String>,
        new_bin: Option<String>,
        qty: i64,
        overflow_rack: i64,
        order_number: Option<String>,
        notes: Option<String>,
    }

    let mut records: Vec<TileRecord> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut skipped = 0usize;

    for (row_idx, row) in rows.enumerate() {
        let line = row_idx + 2;

        let item_number = opt_str(row.get(1)).unwrap_or_default();
        if item_number.is_empty() {
            skipped += 1;
            continue;
        }

        let qty_raw = row.get(5).map(|c| c.to_string()).unwrap_or_default();
        let qty: i64 = match qty_raw.trim().parse::<f64>() {
            Ok(f) => f as i64,
            Err(_) if qty_raw.trim().is_empty() => 0,
            Err(_) => {
                errors.push(format!(
                    "Row {line}: invalid qty '{qty_raw}' for '{item_number}' — skipped"
                ));
                skipped += 1;
                continue;
            }
        };

        let overflow_rack: i64 =
            if opt_str(row.get(6)).unwrap_or_default().to_uppercase() == "Y" {
                1
            } else {
                0
            };

        records.push(TileRecord {
            item_number,
            collection: opt_str(row.get(2)),
            gts_description: opt_str(row.get(3)),
            new_bin: opt_str(row.get(4)),
            qty,
            overflow_rack,
            order_number: opt_str(row.get(7)),
            notes: opt_str(row.get(8)),
        });
    }

    let upserted = records.len();
    let mut tx = state.db.begin().await?;

    for r in &records {
        sqlx::query!(
            r#"
            INSERT INTO tiles
                (branch_id, item_number, collection, gts_description,
                 new_bin, qty, overflow_rack, order_number, notes)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(branch_id, item_number) DO UPDATE SET
                collection      = excluded.collection,
                gts_description = excluded.gts_description,
                new_bin         = excluded.new_bin,
                qty             = excluded.qty,
                overflow_rack   = excluded.overflow_rack,
                order_number    = excluded.order_number,
                notes           = excluded.notes,
                updated_at      = datetime('now')
            "#,
            auth.branch_id,
            r.item_number,
            r.collection,
            r.gts_description,
            r.new_bin,
            r.qty,
            r.overflow_rack,
            r.order_number,
            r.notes,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    tracing::info!(
        "Import: {} upserted, {} skipped, {} errors (branch {})",
        upserted, skipped, errors.len(), auth.branch_id
    );

    let result = ImportResult { upserted, skipped, errors };
    let mut ctx = Context::new();
    ctx.insert("result", &result);
    ctx.insert("auth_user_name", &auth.name);
    ctx.insert("auth_user_role", auth.role.as_str());
    ctx.insert("branch_name", &branch_name(state, auth.branch_id).await?);
    state.render("import/result.html", &ctx)
}

fn opt_str(cell: Option<&calamine::Data>) -> Option<String> {
    cell.map(|c| c.to_string().trim().to_string())
        .filter(|s| !s.is_empty())
}

async fn branch_name(state: &AppState, branch_id: i64) -> Result<String, AppError> {
    sqlx::query!("SELECT name FROM branches WHERE id = ?", branch_id)
        .fetch_optional(&state.db)
        .await?
        .map(|r| r.name)
        .ok_or(AppError::NotFound)
}
