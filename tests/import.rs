mod helpers;
use helpers::*;
use axum::http::StatusCode;
use axum_test::multipart::{MultipartForm, Part};
use sqlx::SqlitePool;

// ── Valid .xlsx upserts tiles into the branch ─────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn valid_xlsx_upserts_tiles(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let bytes = make_xlsx(&[("ITEM-001", 10), ("ITEM-002", 5), ("ITEM-003", 0)]);
    let form = MultipartForm::new()
        .add_part("file", Part::bytes(bytes).file_name("inventory.xlsx"));

    let server = make_server(pool.clone()).await;
    login(&server, "alice@test.com").await;

    let res = server.post("/import").multipart(form).await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM tiles WHERE branch_id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3, "all three tiles should be in the DB");

    let qty = sqlx::query_scalar!("SELECT qty FROM tiles WHERE item_number = 'ITEM-001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qty, 10);
}

// ── .xls extension is rejected ────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn xls_extension_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    // Content doesn't matter — the filename check fires before parsing
    let form = MultipartForm::new()
        .add_part("file", Part::bytes(b"not-real-xls".to_vec()).file_name("inventory.xls"));

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server.post("/import").multipart(form).await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Missing required column header is rejected ────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn missing_required_column_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    // Build an xlsx with the wrong headers (no ITEM NUMBER or QTY in expected positions)
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_string(0, 0, "WRONG").ok();
    ws.write_string(0, 1, "HEADERS").ok();
    ws.write_string(0, 2, "EVERYWHERE").ok();
    let bytes = wb.save_to_buffer().expect("xlsx build");

    let form = MultipartForm::new()
        .add_part("file", Part::bytes(bytes).file_name("bad-headers.xlsx"));

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server.post("/import").multipart(form).await;
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Re-importing the same item number updates qty (upsert, not error) ─────────

#[sqlx::test(migrations = "./migrations")]
async fn reimport_updates_existing_tile(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let server = make_server(pool.clone()).await;
    login(&server, "alice@test.com").await;

    // First import: qty = 10
    let bytes = make_xlsx(&[("ITEM-001", 10)]);
    server
        .post("/import")
        .multipart(MultipartForm::new().add_part("file", Part::bytes(bytes).file_name("v1.xlsx")))
        .await;

    // Second import: same item, qty = 99
    let bytes = make_xlsx(&[("ITEM-001", 99)]);
    let res = server
        .post("/import")
        .multipart(MultipartForm::new().add_part("file", Part::bytes(bytes).file_name("v2.xlsx")))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let qty = sqlx::query_scalar!("SELECT qty FROM tiles WHERE item_number = 'ITEM-001'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qty, 99, "second import should overwrite qty");

    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM tiles WHERE branch_id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "should still be one tile, not two");
}
