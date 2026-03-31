mod helpers;
use helpers::*;
use axum::http::StatusCode;
use axum_test::multipart::{MultipartForm, Part};
use sqlx::SqlitePool;

// ── Chat message length ───────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn chat_message_too_long_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let long_msg = "x".repeat(1_001);
    let res = server
        .post(&format!("/tiles/{tile_id}/chat"))
        .form(&[("message", long_msg.as_str())])
        .await;

    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Refill qty bounds ────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn refill_qty_zero_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "0")])
        .await;

    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn refill_qty_over_limit_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "10001")])
        .await;

    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── Admin user creation validation ───────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn create_user_invalid_email_is_rejected(pool: SqlitePool) {
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;

    let server = make_server(pool).await;
    login(&server, "admin@test.com").await;

    let res = server
        .post("/admin/users")
        .form(&[
            ("name", "Bob"),
            ("email", "not-an-email"),
            ("password", "password123"),
            ("role", "coordinator"),
            ("branch_id", "1"),
        ])
        .await;

    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
async fn create_user_duplicate_email_is_conflict(pool: SqlitePool) {
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;
    seed_user(&pool, 1, "Existing", "existing@test.com", "coordinator").await;

    let server = make_server(pool).await;
    login(&server, "admin@test.com").await;

    let res = server
        .post("/admin/users")
        .form(&[
            ("name", "Bob"),
            ("email", "existing@test.com"),
            ("password", "password123"),
            ("role", "coordinator"),
            ("branch_id", "1"),
        ])
        .await;

    assert_eq!(res.status_code(), StatusCode::CONFLICT);
}

// ── Cross-branch user assignment is rejected ──────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn assign_coordinator_from_other_branch_is_rejected(pool: SqlitePool) {
    let branch_b = seed_branch(&pool, "Branch B").await;
    // Alice is coordinator in branch 1 (NYC)
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    // Bob is coordinator in branch B — different branch
    let bob_id = seed_user(&pool, branch_b, "Bob", "bob@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .put(&format!("/tiles/{tile_id}"))
        .form(&[("sample_coordinator_id", bob_id.to_string().as_str())])
        .await;

    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}

// ── GET /import forbidden for sales reps ──────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn get_import_page_forbidden_for_sales_rep(pool: SqlitePool) {
    seed_user(&pool, 1, "Rep", "rep@test.com", "sales_rep").await;

    let server = make_server(pool).await;
    login(&server, "rep@test.com").await;

    let res = server.get("/import").await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

// ── GET /health requires authentication ──────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn health_requires_auth(pool: SqlitePool) {
    let server = make_server(pool).await;

    let res = server.get("/health").await;
    // Unauthenticated → redirect to login
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/login");
}

#[sqlx::test(migrations = "./migrations")]
async fn health_returns_ok_when_authenticated(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server.get("/health").await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let body: serde_json::Value = serde_json::from_str(&res.text()).unwrap();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], true);
}

// ── xlsx with invalid qty is partially imported (error rows skipped) ──────────

#[sqlx::test(migrations = "./migrations")]
async fn xlsx_bad_qty_row_is_skipped_rest_imported(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    // Make xlsx where row 2 has a good qty and row 3 has "N/A" for qty
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    // Anchor at col 0 so calamine's range origin is column 0, not column 1
    ws.write_string(0, 0, "ROW").ok();
    ws.write_string(0, 1, "ITEM NUMBER").ok();
    ws.write_string(0, 5, "QTY").ok();
    ws.write_string(1, 1, "GOOD-001").ok();
    ws.write_number(1, 5, 5.0).ok();
    ws.write_string(2, 1, "BAD-002").ok();
    ws.write_string(2, 5, "N/A").ok(); // invalid qty — should be skipped
    ws.write_string(3, 1, "GOOD-003").ok();
    ws.write_number(3, 5, 8.0).ok();
    let bytes = wb.save_to_buffer().unwrap();

    let form = MultipartForm::new()
        .add_part("file", Part::bytes(bytes).file_name("test.xlsx"));

    let server = make_server(pool.clone()).await;
    login(&server, "alice@test.com").await;

    let res = server.post("/import").multipart(form).await;
    assert_eq!(res.status_code(), StatusCode::OK);

    // Good rows imported, bad row skipped
    let count = sqlx::query_scalar!("SELECT COUNT(*) FROM tiles WHERE branch_id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2, "GOOD-001 and GOOD-003 should be imported; BAD-002 skipped");
}
