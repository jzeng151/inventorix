mod helpers;
use helpers::*;
use axum::http::StatusCode;
use sqlx::SqlitePool;

// ── GET /tiles/:id — cross-branch access returns 404, not the tile ────────────

#[sqlx::test(migrations = "./migrations")]
async fn get_tile_cross_branch_is_not_found(pool: SqlitePool) {
    let branch_a = 1i64; // seeded by migration (NYC Showroom)
    let branch_b = seed_branch(&pool, "Branch B").await;

    seed_user(&pool, branch_a, "Alice", "alice@test.com", "coordinator").await;
    let tile_b = seed_tile(&pool, branch_b, "TILE-B-001", 10).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server.get(&format!("/tiles/{tile_b}")).await;
    // Branch B tile is invisible to Branch A user — 404 not 403 (avoids existence leak)
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ── PUT /tiles/:id — cross-branch qty update returns 404 ─────────────────────

#[sqlx::test(migrations = "./migrations")]
async fn put_tile_cross_branch_is_not_found(pool: SqlitePool) {
    let branch_a = 1i64;
    let branch_b = seed_branch(&pool, "Branch B").await;

    seed_user(&pool, branch_a, "Alice", "alice@test.com", "coordinator").await;
    let tile_b = seed_tile(&pool, branch_b, "TILE-B-002", 5).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server
        .put(&format!("/tiles/{tile_b}"))
        .form(&[("qty", "99")])
        .await;
    assert_eq!(res.status_code(), StatusCode::NOT_FOUND);
}

// ── GET / — inventory only shows tiles from the authenticated user's branch ───

#[sqlx::test(migrations = "./migrations")]
async fn inventory_only_returns_own_branch_tiles(pool: SqlitePool) {
    let branch_a = 1i64;
    let branch_b = seed_branch(&pool, "Branch B").await;

    seed_user(&pool, branch_a, "Alice", "alice@test.com", "coordinator").await;
    seed_tile(&pool, branch_a, "TILE-A-001", 3).await;
    seed_tile(&pool, branch_a, "TILE-A-002", 7).await;
    seed_tile(&pool, branch_b, "TILE-B-SECRET", 1).await;

    let server = make_server(pool).await;
    login(&server, "alice@test.com").await;

    let res = server.get("/").await;
    assert_eq!(res.status_code(), StatusCode::OK);
    let body = res.text();
    assert!(body.contains("TILE-A-001"), "own tile must appear");
    assert!(body.contains("TILE-A-002"), "own tile must appear");
    assert!(!body.contains("TILE-B-SECRET"), "other branch tile must NOT appear");
}
