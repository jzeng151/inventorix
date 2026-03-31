mod helpers;
use helpers::*;
use axum::http::StatusCode;
use sqlx::SqlitePool;

// ── Deactivating a user immediately invalidates their session ─────────────────

#[sqlx::test(migrations = "./migrations")]
async fn deactivate_user_invalidates_session(pool: SqlitePool) {
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;
    let bob_id = seed_user(&pool, 1, "Bob", "bob@test.com", "coordinator").await;

    // Bob logs in and confirms access
    let bob_server = make_server(pool.clone()).await;
    login(&bob_server, "bob@test.com").await;
    let res = bob_server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::OK, "Bob should be logged in");

    // Admin deactivates Bob
    let admin_server = make_server(pool.clone()).await;
    login(&admin_server, "admin@test.com").await;
    let res = admin_server
        .post(&format!("/admin/users/{bob_id}/deactivate"))
        .await;
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER); // redirect to /admin

    // Bob's session should now be gone — next request redirects to login
    let res = bob_server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::SEE_OTHER);
    assert_eq!(res.header("location"), "/login");
}

// ── Admin cannot deactivate their own account ────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn cannot_deactivate_own_account(pool: SqlitePool) {
    let admin_id = seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;

    let server = make_server(pool).await;
    login(&server, "admin@test.com").await;

    let res = server
        .post(&format!("/admin/users/{admin_id}/deactivate"))
        .await;

    // Validation error — cannot self-deactivate
    assert_eq!(res.status_code(), StatusCode::UNPROCESSABLE_ENTITY);
}
