# Inventorix — Claude Code Project Instructions

Internal inventory management system for a tile sample warehouse. Replaces Excel + MS Teams + Salepad
with a single Windows desktop app. NYC branch pilot. Multi-branch schema from day 1.

**Full context:** Read `ARCHITECTURE.md`, `DESIGN-DOC.md`, `ROADMAP.md`, `TODOS.md` before making
any structural changes. The CEO plan, eng review, and design review are in
`~/.gstack/projects/jzeng151-inventorix/` if you need deeper context on why decisions were made.

---

## Tech Stack (do not deviate without a team decision)

- **Runtime:** Rust — single binary. No Python, no Node, no Electron.
- **Web server:** Axum. Spawned inside Tauri via `tokio::spawn`.
- **Desktop shell:** Tauri (Edge WebView2). Single `.msi` installer.
- **Templates:** Tera (server-rendered). HTMX for partial swaps. No React, no Vue.
- **Database:** SQLite (WAL mode). Driver: SQLx with compile-time query verification.
- **Sessions:** tower-sessions + SQLite store. No JWT.
- **Excel read:** calamine. Excel write: rust_xlsxwriter.
- **Passwords:** argon2.
- **Errors:** thiserror (`AppError` enum implementing `IntoResponse`).
- **Logging:** tracing.
- **Migrations:** sqlx-cli. All schema changes through migration files.
- **Background jobs:** `tokio::spawn` loops. No scheduler crate.

---

## Naming Conventions

- **In code:** `tile` (struct `Tile`, table `tiles`, variable `tile`)
- **In UI copy:** "Sample" — e.g., "Sample Coordinator", "sample inventory"
- **Roles in code:** `admin`, `coordinator`, `sales_rep` (snake_case strings in DB)
- **Roles in UI:** "Admin", "Sample Coordinator", "Sales Rep"
- **Routes:** kebab-case (e.g., `/refill/:id/approve`, `/admin/users`)
- **Rust files:** snake_case. Template files: kebab-case.

---

## Key Patterns — Read Before Writing Any Handler

### AuthUser extractor — mandatory on every protected handler
Branch isolation is compiler-enforced. Every protected handler must accept `AuthUser`.
`branch_id` comes from the session only — never from request body or URL params.

```rust
// Every protected handler looks like this:
async fn get_tile(
    State(state): State<AppState>,
    auth: AuthUser,              // ← required, or it won't compile
    Path(tile_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    // Always scope queries to auth.branch_id
    let tile = sqlx::query_as!(Tile,
        "SELECT * FROM tiles WHERE id = ? AND branch_id = ?",
        tile_id, auth.branch_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    // ...
}
```

### AppError — use it, don't unwrap
```rust
// Good
.ok_or(AppError::NotFound)?
.map_err(|_| AppError::Conflict("Import already running".into()))?

// Never
.unwrap()
.expect("this shouldn't fail")
```

### Import lock — check before any Excel import
```rust
let mut lock = state.import_lock.lock().await;
if *lock { return Err(AppError::Conflict("Import already in progress".into())); }
*lock = true;
// ... do import ...
*lock = false;
```

### WebSocket broadcast — always scope to branch_id
```rust
state.ws_manager.broadcast(auth.branch_id, WsEvent::InventoryUpdate { tile_id, new_qty });
```

### Background jobs — tokio::spawn loop, not a crate
```rust
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        if let Err(e) = check_expired_timers(&pool).await {
            tracing::error!("Timer check failed: {e}");
        }
    }
});
```

### SQLx queries — always use `sqlx::query!` or `sqlx::query_as!`
Never use raw string queries with runtime binding. Compile-time verification catches errors before they ship.

```rust
// Good
sqlx::query_as!(Tile, "SELECT * FROM tiles WHERE branch_id = ?", auth.branch_id)

// Never
sqlx::query("SELECT * FROM tiles WHERE branch_id = ?")
```

### Excel import — always wrap upserts in a transaction
```rust
let mut tx = state.db.begin().await?;
for row in rows {
    sqlx::query!("INSERT OR REPLACE INTO tiles ...", ...).execute(&mut *tx).await?;
}
tx.commit().await?;
// 700 rows in one transaction: ~100-500x faster than autocommit per row
```

### Salesforce — always use the trait, never call reqwest directly
```rust
// In handler:
state.salesforce.notify_refill(&payload).await
    .unwrap_or_else(|e| tracing::error!("Salesforce failed: {e}"));
// Fail open — log the error, don't 500 the approval endpoint
```

---

## Role Enforcement Rules

These are enforced at the API level, not just the UI. The UI hides elements; the API enforces.

| Endpoint | Forbidden role | Returns |
|----------|---------------|---------|
| `POST /refill/:id/approve` | `coordinator` | 403 |
| `POST /tiles/:id` (qty update) | `sales_rep` | 403 |
| `POST /tiles/:id/refill` | `sales_rep` | 403 |
| `POST /refill/:id/fulfill` | `sales_rep` | 403 |
| `GET /admin` and all admin routes | `coordinator`, `sales_rep` | 403 |

Branch isolation: all tile queries must include `AND branch_id = auth.branch_id` unless the user is an admin. Admin bypasses branch filter.

---

## Database Rules

- **Never DROP and recreate tables.** All schema changes go through `sqlx migrate add <name>`.
- **Never skip the busy_timeout.** SQLite WAL mode requires it: `.busy_timeout(Duration::from_secs(5))`.
- **Always JOIN users when fetching tiles for display.** N+1 (fetching coordinator/rep per row) will blow the 500ms inventory load target. Use one query with JOINs or a single `SELECT * FROM users WHERE branch_id = ?` pre-fetch.
- **`idx_refill_timer` must exist.** The 5-minute timer loop queries this index. Without it: full table scan every 5 minutes.

---

## Testing Rules

Framework: `cargo test` + `#[sqlx::test]` (fresh in-memory SQLite per test, migrations auto-run).

**P0 tests (must pass before any merge):**
- `GET /tiles/:id` with tile in branch B, user in branch A → 403
- `PUT /tiles/:id` with tile in branch B, user in branch A → 403
- `GET /tiles` returns only tiles for authenticated user's branch
- `POST /refill/:id/approve` with coordinator role → 403

Test fixtures live in `tests/fixtures/`. See `ARCHITECTURE.md` for the full test list.

Strategy: TDD per lane — tests written alongside feature code, not after.

---

## Dev Workflow

**Template iteration (fast):**
```bash
cargo watch -x 'run --bin inventorix-server'
# Axum runs standalone at localhost:3000
# Template changes reload without Tauri recompile
```

**Full Tauri bundle (for Tauri-specific testing only):**
```bash
cargo tauri dev
```

**Migrations:**
```bash
sqlx migrate run    # applies pending migrations
sqlx migrate add <name>    # creates new migration file
```

**Environment:**
```bash
cp .env.example .env
# Set SALESFORCE_MODE=mock for local dev — never use live unless testing Salesforce specifically
```

---

## UI Rules (Tera templates)

- Use CSS custom properties from `static/css/app.css`. Never hardcode hex colors in templates.
- Role-aware rendering: render different HTML for different roles, don't use `display:none` to hide elements.
- Every table row: `tabindex="0"`, click handler on the `<tr>` opens tile detail.
- Health badges: always include `aria-label="Critical: 2 in stock"` — color alone is inaccessible.
- Chat messages: `aria-live="polite"` on the message list container.
- The inventory table has no pagination — 700 rows, sorted by health (critical first).

See `DESIGN-DOC.md` for the complete design system, all component states, and empty state copy.

---

## What NOT to Do

- Do not add a scheduler crate (tokio-cron-scheduler, cron, etc.). Use `tokio::spawn` loops.
- Do not use JWT for sessions. Sessions are invalidated server-side by deleting DB rows.
- Do not use polling for inventory updates. WebSocket broadcast on mutation.
- Do not use client-side timers for the refill countdown. Update via WebSocket push.
- Do not write raw `sqlx::query(...)` strings. Use `sqlx::query!` macros.
- Do not call Salesforce directly from handlers. Go through the `SalesforceClient` trait.
- Do not modal dialogs for confirmations. Use inline confirmation patterns.
- Do not add pagination to the inventory table. 700 rows scrolls fine.
- Do not `.unwrap()` in handlers. Use `?` and `AppError`.
- Do not share pricing data with Salesforce. The outbound payload spec is in `ARCHITECTURE.md`.
- Do not commit `.env`. Only `.env.example` goes in the repo.

---

## Lane Structure (1-week build)

| Lane | Owner | Scope | Critical dependency |
|------|-------|-------|-------------------|
| A | — | DB models, sqlx migrations, AuthUser extractor, AppState, AppError, CONTRIBUTING.md | Ships EOD Day 1 — everyone else blocks on this |
| B | — | Inventory UI, Tera templates, search, color coding, tile detail | Depends on Lane A |
| C | — | WebSocket ConnectionManager, broadcast, chat routes | Depends on Lane A |
| D | — | Excel import (calamine), export (rust_xlsxwriter), HTML digest | Depends on Lane A + Lane E stubs (Day 2) |
| E | — | Tauri shell, IPC commands, .msi installer | Day 2: ship stubs for pick_excel_file + open_digest_in_browser |
| F | — | Refill workflow, Salesforce mock/live, timer loop | Depends on Lane A |

**Hard dependency:** Lane E must ship `pick_excel_file` and `open_digest_in_browser` stubs by EOD Day 2, or Lane D is blocked.
