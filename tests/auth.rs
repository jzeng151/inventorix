mod helpers;
use helpers::*;
use axum::http::StatusCode;
use sqlx::SqlitePool;

// ── Valid credentials → redirect to inventory ─────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn valid_login_redirects_to_inventory(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let server = make_server(pool).await;

    let res = server
        .post("/auth/login")
        .form(&[("email", "alice@test.com"), ("password", TEST_PASSWORD)])
        .await;

    // 303 redirect to /
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/");
}

// ── Wrong password → 200 with error page (no timing enumeration) ─────────────

#[sqlx::test(migrations = "./migrations")]
async fn wrong_password_returns_error_page(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let server = make_server(pool).await;

    let res = server
        .post("/auth/login")
        .form(&[("email", "alice@test.com"), ("password", "wrong-password")])
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    assert!(res.text().contains("Incorrect"), "error message must appear");
}

// ── Unknown email → same 200 response (no user enumeration) ──────────────────

#[sqlx::test(migrations = "./migrations")]
async fn unknown_email_returns_same_error_as_wrong_password(pool: SqlitePool) {
    let server = make_server(pool).await;

    let res = server
        .post("/auth/login")
        .form(&[("email", "nobody@test.com"), ("password", "anything")])
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
}

// ── Unauthenticated request to / redirects to /login ─────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn unauthenticated_root_redirects_to_login(pool: SqlitePool) {
    let server = make_server(pool).await;

    let res = server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/login");
}

// ── Logout flushes session — subsequent / redirects to login ─────────────────

#[sqlx::test(migrations = "./migrations")]
async fn logout_invalidates_session(pool: SqlitePool) {
    seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    // Confirm logged in
    let res = server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::OK);

    // Log out
    server.post("/auth/logout").await;

    // Should redirect to login now
    let res = server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/login");
}
