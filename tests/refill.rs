mod helpers;
use helpers::*;
use axum::http::StatusCode;
use sqlx::SqlitePool;

// ── Coordinator can request a refill ─────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn coordinator_can_request_refill(pool: SqlitePool) {
    let user_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let _tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;
    let tile_id = _tile_id;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "5")])
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);
    assert_eq!(res.header("hx-refresh"), "true");

    let _ = user_id;
}

// ── Duplicate active refill request is a conflict ────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn duplicate_active_refill_is_conflict(pool: SqlitePool) {
    let user_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;
    seed_refill_request(&pool, tile_id, user_id, "pending").await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "3")])
        .await;

    assert_eq!(res.status_code(), StatusCode::CONFLICT);
}

// ── Admin can approve a pending refill ───────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn admin_can_approve_pending_refill(pool: SqlitePool) {
    let coordinator_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;
    let refill_id = seed_refill_request(&pool, tile_id, coordinator_id, "pending").await;

    let server = make_server(pool.clone()).await;
    login(&server, "admin@test.com").await;

    let res = server
        .post(&format!("/refill/{refill_id}/approve"))
        .await;

    assert_eq!(res.status_code(), StatusCode::OK);

    let row = sqlx::query!("SELECT status FROM refill_requests WHERE id = ?", refill_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "approved");
}

// ── Approving an already-approved request is a conflict ──────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn approve_already_approved_is_conflict(pool: SqlitePool) {
    let coordinator_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;
    let refill_id = seed_refill_request(&pool, tile_id, coordinator_id, "approved").await;

    let server = make_server(pool).await;
    login(&server, "admin@test.com").await;

    let res = server
        .post(&format!("/refill/{refill_id}/approve"))
        .await;

    assert_eq!(res.status_code(), StatusCode::CONFLICT);
}

// ── Fulfilling a pending (not yet approved) request is a conflict ─────────────

#[sqlx::test(migrations = "./migrations")]
async fn fulfill_requires_approved_not_pending(pool: SqlitePool) {
    let coordinator_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;
    let refill_id = seed_refill_request(&pool, tile_id, coordinator_id, "pending").await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .post(&format!("/refill/{refill_id}/fulfill"))
        .await;

    assert_eq!(res.status_code(), StatusCode::CONFLICT);
}

// ── Full state machine: pending → approved → fulfilled ───────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn full_state_machine_request_approve_fulfill(pool: SqlitePool) {
    seed_user(&pool, 1, "Coordinator", "coord@test.com", "coordinator").await;
    seed_user(&pool, 1, "Admin", "admin@test.com", "admin").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 10).await;

    // 1. Coordinator requests refill
    let coord_server = make_server(pool.clone()).await;
    login(&coord_server, "coord@test.com").await;
    let res = coord_server
        .post(&format!("/tiles/{tile_id}/refill"))
        .form(&[("qty", "5")])
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let row = sqlx::query!("SELECT id, status FROM refill_requests WHERE tile_id = ?", tile_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "pending");
    let refill_id = row.id;

    // 2. Admin approves
    let admin_server = make_server(pool.clone()).await;
    login(&admin_server, "admin@test.com").await;
    let res = admin_server
        .post(&format!("/refill/{refill_id}/approve"))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    // 3. Coordinator fulfills
    let res = coord_server
        .post(&format!("/refill/{refill_id}/fulfill"))
        .await;
    assert_eq!(res.status_code(), StatusCode::OK);

    let row = sqlx::query!("SELECT status FROM refill_requests WHERE id = ?", refill_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "fulfilled");
}

// ── Timer job marks expired refill requests ───────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn expired_timer_marks_refill_expired(pool: SqlitePool) {
    let user_id = seed_user(&pool, 1, "Alice", "alice@test.com", "coordinator").await;
    let tile_id = seed_tile(&pool, 1, "TILE-001", 5).await;
    let refill_id = seed_expired_refill_request(&pool, tile_id, user_id).await;

    let state = inventorix_server::AppState::for_test(pool.clone());
    inventorix_server::jobs::timers::check_expired_timers(&state)
        .await
        .expect("timer check failed");

    let row = sqlx::query!("SELECT status FROM refill_requests WHERE id = ?", refill_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row.status, "expired");
}
