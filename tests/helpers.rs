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
