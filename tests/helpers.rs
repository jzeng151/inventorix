use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use axum_test::TestServer;
use inventorix_server::{routes::build_router, AppState};
use sqlx::SqlitePool;

pub const TEST_PASSWORD: &str = "test-password-123";

/// Build a TestServer with cookie persistence from a test pool.
pub async fn make_server(pool: SqlitePool) -> TestServer {
    let state = AppState::for_test(pool);
    let router = build_router(state).await;
    TestServer::builder()
        .save_cookies()
        .build(router)
}

/// Hash a password for seeding test users.
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hash failed")
        .to_string()
}

/// Seed a branch, returning its id. Branch 1 (NYC Showroom) is seeded by the migration.
pub async fn seed_branch(pool: &SqlitePool, name: &str) -> i64 {
    sqlx::query!(
        "INSERT INTO branches (name, territory) VALUES (?, ?) RETURNING id",
        name, name
    )
    .fetch_one(pool)
    .await
    .expect("seed_branch")
    .id
}

/// Seed a user, returning their id.
pub async fn seed_user(
    pool: &SqlitePool,
    branch_id: i64,
    name: &str,
    email: &str,
    role: &str,
) -> i64 {
    let hash = hash_password(TEST_PASSWORD);
    sqlx::query!(
        "INSERT INTO users (branch_id, name, role, email, password_hash) VALUES (?, ?, ?, ?, ?) RETURNING id",
        branch_id, name, role, email, hash
    )
    .fetch_one(pool)
    .await
    .expect("seed_user")
    .id
}

/// Seed a tile in a branch, returning its id.
pub async fn seed_tile(pool: &SqlitePool, branch_id: i64, item_number: &str, qty: i64) -> i64 {
    sqlx::query!(
        "INSERT INTO tiles (branch_id, item_number, qty) VALUES (?, ?, ?) RETURNING id",
        branch_id, item_number, qty
    )
    .fetch_one(pool)
    .await
    .expect("seed_tile")
    .id
}

/// Log in via the test server. Returns after the redirect — subsequent requests carry the session.
pub async fn login(server: &TestServer, email: &str) {
    server
        .post("/auth/login")
        .form(&[("email", email), ("password", TEST_PASSWORD)])
        .await;
}

/// Seed a refill request in a given status (e.g. "pending", "approved").
/// Timer is set 48h in the future so it won't expire naturally.
pub async fn seed_refill_request(
    pool: &SqlitePool,
    tile_id: i64,
    requested_by: i64,
    status: &str,
) -> i64 {
    sqlx::query!(
        r#"INSERT INTO refill_requests
               (tile_id, requested_by, qty_requested, status, timer_expires_at)
           VALUES (?, ?, 5, ?, datetime('now', '+48 hours'))
           RETURNING id"#,
        tile_id, requested_by, status
    )
    .fetch_one(pool)
    .await
    .expect("seed_refill_request")
    .id
}

/// Seed a refill request whose timer has already expired (for timer-job tests).
pub async fn seed_expired_refill_request(
    pool: &SqlitePool,
    tile_id: i64,
    requested_by: i64,
) -> i64 {
    sqlx::query!(
        r#"INSERT INTO refill_requests
               (tile_id, requested_by, qty_requested, status, timer_expires_at)
           VALUES (?, ?, 5, 'pending', datetime('now', '-1 hour'))
           RETURNING id"#,
        tile_id, requested_by
    )
    .fetch_one(pool)
    .await
    .expect("seed_expired_refill_request")
    .id
}

/// Build a minimal valid .xlsx file using the column layout expected by the import handler.
/// Col 0: (ignored), Col 1: ITEM NUMBER, Col 2: COLLECTION, Col 3: GTS DESCRIPTION,
/// Col 4: NEW BIN, Col 5: QTY, Col 6: Overflow Rack, Col 7: Order #, Col 8: Notes
pub fn make_xlsx(items: &[(&str, i64)]) -> Vec<u8> {
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let ws = wb.add_worksheet();
    ws.write_string(0, 0, "ROW").ok();
    ws.write_string(0, 1, "ITEM NUMBER").ok();
    ws.write_string(0, 2, "COLLECTION").ok();
    ws.write_string(0, 3, "GTS DESCRIPTION").ok();
    ws.write_string(0, 4, "NEW BIN").ok();
    ws.write_string(0, 5, "QTY").ok();
    ws.write_string(0, 6, "Overflow Rack").ok();
    ws.write_string(0, 7, "Order #").ok();
    ws.write_string(0, 8, "Notes").ok();
    for (i, (item_number, qty)) in items.iter().enumerate() {
        let row = (i + 1) as u32;
        ws.write_string(row, 1, *item_number).ok();
        ws.write_number(row, 5, *qty as f64).ok();
    }
    wb.save_to_buffer().expect("make_xlsx failed")
}
