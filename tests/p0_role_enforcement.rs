mod helpers;
use helpers::*;
use axum::http::StatusCode;
use sqlx::SqlitePool;

// ── POST /refill/:id/approve — coordinator is forbidden ───────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn approve_refill_coordinator_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;

    let coordinator_id = seed_user(&pool, branch_id, "Coord", "coord@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, branch_id, "TILE-001", 2).await;

    // Create a pending refill request
    let refill_id = sqlx::query!(
        r#"INSERT INTO refill_requests (tile_id, requested_by, qty_requested, timer_expires_at)
           VALUES (?, ?, 5, datetime('now', '+48 hours')) RETURNING id"#,
        tile_id, coordinator_id
    )
    .fetch_one(&pool)
    .await
    .expect("seed refill")
    .id;

    let server = make_server(pool).await;
    login(&server, "coord@test.com").await;

    let res = server.post(&format!("/refill/{refill_id}/approve")).await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

// ── POST /tiles/:id — sales_rep cannot update qty ────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn update_qty_sales_rep_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;

    seed_user(&pool, branch_id, "Rep", "rep@test.com", "sales_rep").await;
    let tile_id = seed_tile(&pool, branch_id, "TILE-002", 5).await;

    let server = make_server(pool).await;
    login(&server, "rep@test.com").await;

    let res = server
        .put(&format!("/tiles/{tile_id}"))
        .form(&[("qty", "99")])
        .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

// ── POST /tiles/:id/refill — sales_rep cannot request a refill ───────────────

#[sqlx::test(migrations = "./migrations")]
async fn request_refill_sales_rep_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;

    seed_user(&pool, branch_id, "Rep", "rep@test.com", "sales_rep").await;
    let tile_id = seed_tile(&pool, branch_id, "TILE-003", 0).await;

    let server = make_server(pool).await;
    login(&server, "rep@test.com").await;

    let res = server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "5")])
        .await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

// ── POST /refill/:id/fulfill — sales_rep cannot fulfill ──────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn fulfill_refill_sales_rep_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;

    let coordinator_id = seed_user(&pool, branch_id, "Coord", "coord@test.com", "coordinator").await;
    seed_user(&pool, branch_id, "Rep", "rep@test.com", "sales_rep").await;
    let tile_id = seed_tile(&pool, branch_id, "TILE-004", 0).await;

    let refill_id = sqlx::query!(
        r#"INSERT INTO refill_requests (tile_id, requested_by, qty_requested, status, timer_expires_at)
           VALUES (?, ?, 5, 'approved', datetime('now', '+48 hours')) RETURNING id"#,
        tile_id, coordinator_id
    )
    .fetch_one(&pool)
    .await
    .expect("seed refill")
    .id;

    let server = make_server(pool).await;
    login(&server, "rep@test.com").await;

    let res = server.post(&format!("/refill/{refill_id}/fulfill")).await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

// ── GET /admin — non-admin is forbidden ──────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn admin_page_coordinator_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;
    seed_user(&pool, branch_id, "Coord", "coord@test.com", "coordinator").await;

    let server = make_server(pool).await;
    login(&server, "coord@test.com").await;

    let res = server.get("/admin").await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
async fn admin_page_sales_rep_is_forbidden(pool: SqlitePool) {
    let branch_id = 1i64;
    seed_user(&pool, branch_id, "Rep", "rep@test.com", "sales_rep").await;

    let server = make_server(pool).await;
    login(&server, "rep@test.com").await;

    let res = server.get("/admin").await;
    assert_eq!(res.status_code(), StatusCode::FORBIDDEN);
}
