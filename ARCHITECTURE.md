# Inventorix — Architecture

> Internal inventory management system for a tile sample warehouse.
> Replaces Excel + MS Teams + Salepad with a single Windows desktop app.
> NYC branch pilot, multi-branch schema from day 1.

---

## Tech Stack

| Layer | Technology | Why |
|-------|-----------|-----|
| Desktop shell | Tauri (Rust) | Single `.msi` installer, Edge WebView2 (pre-installed on Windows 10/11), no Electron, no Python runtime |
| Web server | Axum (Rust) | Spawned inside the same Tauri process via `tokio::spawn`. No sidecar, no NSSM |
| Database driver | SQLx (async, Rust) | Compile-time query verification, `#[sqlx::test]` for isolated test DBs |
| Database | SQLite (WAL mode) | Single-file, no server, safe for ~10 concurrent users, upgrade path to Postgres |
| Templates | Tera | Jinja2-compatible, server-rendered HTML. No React, no JS framework |
| Dynamic updates | HTMX | Partial HTML swaps over HTTP. Chat, refill status, inventory qty — no client-side state |
| Real-time | WebSocket (Axum) | Branch-scoped broadcast for chat messages and inventory mutations |
| Excel import | calamine | Rust, zero-copy xlsx reader |
| Excel export | rust_xlsxwriter | Rust, generates .xlsx |
| Password hashing | argon2 | Industry standard. ~100-200ms on i3, within 300ms login target |
| Sessions | tower-sessions + SQLite store | True server-side sessions. Invalidation is a DB delete, immediate |
| HTTP client | reqwest | Salesforce outbound calls |
| Error handling | thiserror | `AppError` enum implementing `IntoResponse`. Compiler-enforced |
| Logging | tracing | Structured, async-safe |
| Migrations | sqlx-cli | `sqlx migrate run` on startup. All schema changes through migrations |
| Background jobs | tokio::spawn loop | No scheduler crate. `loop { sleep(5min); check_expired_timers() }` |

**Minimum hardware:** Windows 10, 4 GB RAM, Intel i3 (or equivalent). Edge WebView2 pre-installed on Windows 10 21H1+.

**Not in the stack:** Python, Node, NSSM, Electron, polling-based inventory updates, client-side timers, JWT.

---

## Database Schema

### `branches`
```sql
CREATE TABLE branches (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL,
  territory  TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### `users`
```sql
CREATE TABLE users (
  id            INTEGER PRIMARY KEY,
  branch_id     INTEGER NOT NULL REFERENCES branches(id),
  name          TEXT NOT NULL,
  role          TEXT NOT NULL CHECK(role IN ('admin', 'coordinator', 'sales_rep')),
  email         TEXT NOT NULL UNIQUE,
  password_hash TEXT NOT NULL,
  territory     TEXT,
  created_at    TEXT NOT NULL DEFAULT (datetime('now')),
  is_active     INTEGER NOT NULL DEFAULT 1
);
```

### `tiles`
```sql
CREATE TABLE tiles (
  id                    INTEGER PRIMARY KEY,
  branch_id             INTEGER NOT NULL REFERENCES branches(id),
  item_number           TEXT NOT NULL,
  collection            TEXT,
  gts_description       TEXT,
  new_bin               TEXT,
  qty                   INTEGER NOT NULL DEFAULT 0,
  overflow_rack         INTEGER NOT NULL DEFAULT 0,  -- boolean
  order_number          TEXT,
  notes                 TEXT,
  sample_coordinator_id INTEGER REFERENCES users(id),
  sales_rep_id          INTEGER REFERENCES users(id),
  low_inventory_threshold INTEGER NOT NULL DEFAULT 5,
  created_at            TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at            TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE(branch_id, item_number)
);
```

### `chat_messages`
```sql
CREATE TABLE chat_messages (
  id         INTEGER PRIMARY KEY,
  tile_id    INTEGER NOT NULL REFERENCES tiles(id),
  sender_id  INTEGER NOT NULL REFERENCES users(id),
  message    TEXT NOT NULL,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```
> Chat is scoped to a tile. No global chat. Permanent audit log — visible to Admin.

### `refill_requests`
```sql
CREATE TABLE refill_requests (
  id              INTEGER PRIMARY KEY,
  tile_id         INTEGER NOT NULL REFERENCES tiles(id),
  requested_by    INTEGER NOT NULL REFERENCES users(id),
  approved_by     INTEGER REFERENCES users(id),
  fulfilled_by    INTEGER REFERENCES users(id),
  qty_requested   INTEGER NOT NULL,
  status          TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN ('pending', 'approved', 'fulfilled', 'expired')),
  requested_at    TEXT NOT NULL DEFAULT (datetime('now')),
  approved_at     TEXT,
  fulfilled_at    TEXT,
  timer_expires_at TEXT NOT NULL  -- 48h from requested_at, persisted for restart recovery
);

CREATE INDEX idx_refill_timer ON refill_requests(status, timer_expires_at);
```
> Timer state persists to DB. On server restart, recompute remaining time from `timer_expires_at`. If already past, mark expired immediately.

### `inventory_events`
```sql
CREATE TABLE inventory_events (
  id         INTEGER PRIMARY KEY,
  tile_id    INTEGER NOT NULL REFERENCES tiles(id),
  event_type TEXT NOT NULL CHECK(event_type IN ('import', 'manual_edit', 'refill_fulfilled')),
  old_qty    INTEGER NOT NULL,
  new_qty    INTEGER NOT NULL,
  user_id    INTEGER NOT NULL REFERENCES users(id),
  notes      TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```
> Event log for future trend analytics. Populated from day 1, UI in Phase 3.

### SQLite connection options (required)
```rust
SqliteConnectOptions::new()
    .filename(&db_path)
    .journal_mode(SqliteJournalMode::Wal)
    .busy_timeout(Duration::from_secs(5))  // mandatory — prevents SQLITE_BUSY under concurrent writes
```

---

## Folder Structure

```
inventorix/
├── src-tauri/                  # Tauri desktop shell
│   ├── src/
│   │   └── main.rs             # Tauri setup, spawns Axum via tokio::spawn, IPC commands
│   ├── tauri.conf.json
│   └── Cargo.toml
│
├── src/                        # Axum web server
│   ├── main.rs                 # Server entrypoint, router, AppState init, background jobs
│   ├── state.rs                # AppState: SqlitePool, import_lock, ws_manager, config
│   ├── error.rs                # AppError enum (thiserror) + IntoResponse impl
│   │
│   ├── auth/
│   │   ├── mod.rs
│   │   ├── extractor.rs        # AuthUser: FromRequestParts — compiler-enforced auth
│   │   └── routes.rs           # POST /auth/login, POST /auth/logout
│   │
│   ├── models/                 # SQLx structs matching DB tables
│   │   ├── tile.rs
│   │   ├── user.rs
│   │   ├── refill_request.rs
│   │   ├── chat_message.rs
│   │   └── branch.rs
│   │
│   ├── routes/
│   │   ├── mod.rs              # Router assembly
│   │   ├── tiles.rs            # GET /, GET /tiles/:id, PUT /tiles/:id
│   │   ├── import.rs           # POST /import (Excel → DB)
│   │   ├── export.rs           # POST /export (DB → Excel + HTML digest)
│   │   ├── refill.rs           # POST /tiles/:id/refill, /approve, /fulfill
│   │   ├── chat.rs             # POST /tiles/:id/chat (HTMX partial response)
│   │   ├── admin.rs            # GET/POST /admin, user management
│   │   └── health.rs           # GET /health
│   │
│   ├── ws/
│   │   └── manager.rs          # ConnectionManager: Arc<DashMap<branch_id, Vec<Sender>>>
│   │
│   ├── jobs/
│   │   └── timers.rs           # check_expired_timers(), daily_backup()
│   │
│   └── salesforce/
│       └── client.rs           # SalesforceClient trait, MockClient, LiveClient
│
├── templates/                  # Tera templates
│   ├── base.html               # Layout: top bar, sub-bar, main slot
│   ├── login.html
│   ├── inventory/
│   │   ├── table.html          # Full inventory page
│   │   └── row.html            # HTMX partial — single row swap on qty update
│   ├── tiles/
│   │   └── detail.html         # Two-column: tile fields + chat
│   ├── import.html
│   ├── export.html             # EOD digest preview
│   └── admin/
│       └── users.html
│
├── static/
│   ├── css/
│   │   └── app.css             # CSS custom properties (design tokens) + Inter font
│   └── js/
│       └── htmx.min.js
│
├── migrations/
│   └── 20260330_initial.sql    # All 6 tables + indexes in one migration
│
├── tests/
│   ├── fixtures/
│   │   ├── nyc_inventory.xlsx  # Valid 700-row test file
│   │   ├── bad_columns.xlsx    # Missing QTY column
│   │   ├── duplicates.xlsx     # 5 duplicate item_number rows
│   │   └── merged_cells.xlsx   # Merged header cells
│   ├── auth_tests.rs
│   ├── tile_tests.rs           # Branch isolation (P0 security)
│   ├── import_tests.rs
│   ├── refill_tests.rs
│   ├── ws_tests.rs
│   └── admin_tests.rs
│
├── Cargo.toml
├── Cargo.lock
├── .env.example                # SALESFORCE_MODE, DB_PATH, SESSION_SECRET
├── ARCHITECTURE.md
├── DESIGN-DOC.md
├── ROADMAP.md
├── TODOS.md
└── README.md
```

---

## API Routes

### Auth
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/login` | Public | Login page |
| POST | `/auth/login` | Public | Validate credentials, set session cookie |
| POST | `/auth/logout` | Any | Delete session from DB, clear cookie |

### Inventory
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/` | Any | Inventory table (branch-scoped, sorted by health) |
| GET | `/tiles/:id` | Any | Tile detail: fields + chat + refill status |
| PUT | `/tiles/:id` | Coordinator, Admin | Update qty, notes, assigned users |

### Import / Export
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/import` | Coordinator, Admin | Import page |
| POST | `/import` | Coordinator, Admin | Upload .xlsx → upsert tiles. Returns 409 if import already running. |
| POST | `/export` | Coordinator, Admin | Generate .xlsx + HTML digest. Idempotent per calendar day. |

### Refill Workflow
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| POST | `/tiles/:id/refill` | Coordinator, Admin | Create refill request. Starts 48h timer. |
| POST | `/refill/:id/approve` | Sales Rep, Admin | Approve. Triggers Salesforce call. 403 for Coordinator. |
| POST | `/refill/:id/fulfill` | Coordinator, Admin | Mark fulfilled. Sets fulfilled_by + fulfilled_at. |

### Chat
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| POST | `/tiles/:id/chat` | Any | Send message. Returns HTMX partial (new message HTML). |

### WebSocket
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/ws/:branch_id` | Any (authenticated) | Persistent connection. Receives: chat events, inventory mutations, refill status changes. |

### Admin
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/admin` | Admin | User management panel |
| POST | `/admin/users` | Admin | Create user with role + branch |
| PUT | `/admin/users/:id` | Admin | Update user (role, branch, name) |
| POST | `/admin/users/:id/deactivate` | Admin | Deactivate. Invalidates all active sessions immediately. |

### Health
| Method | Route | Role | Description |
|--------|-------|------|-------------|
| GET | `/health` | Internal | DB connectivity, last timer job run, active WebSocket count |

---

## Key Design Decisions

### 1. Single binary — Axum inside Tauri
Tauri spawns the Axum server via `tokio::spawn` in the `setup` hook. The WebView2 window loads `http://localhost:{PORT}`. One process, one `.msi` installer, no NSSM, no Python runtime. `TcpListener::bind` runs before the spawn to guarantee the port is listening when WebView2 navigates.

### 2. AuthUser extractor — compiler-enforced branch isolation
Every protected handler receives `AuthUser` via Axum's `FromRequestParts`. `AuthUser` carries `id`, `role`, and `branch_id` (read from the session — never from the request body). If you forget to add it to a handler, it won't compile. Branch isolation is not optional.

```rust
pub struct AuthUser { pub id: i64, pub role: Role, pub branch_id: i64 }
impl<S: Send + Sync> FromRequestParts<S> for AuthUser { ... }
```

### 3. Import lock — Arc<Mutex<bool>> in AppState
Concurrent Excel imports would corrupt tile data. The lock is a simple `Arc<Mutex<bool>>` in `AppState`. `POST /import` tries to acquire it, returns 409 if already held, releases on completion or error. No distributed locking needed — single process.

### 4. Sessions — tower-sessions + SQLite store
JWT was explicitly rejected: no session invalidation. When admin deactivates a user, `DELETE FROM sessions WHERE user_id = ?`. Immediate effect. Session table lives in the same SQLite DB.

### 5. Background jobs — tokio::spawn loops
No scheduler crate. Two jobs:

```rust
// Refill timer expiry — every 5 minutes
tokio::spawn(async move {
    loop {
        tokio::time::sleep(Duration::from_secs(300)).await;
        check_expired_timers(&pool).await;
    }
});

// Daily backup
tokio::spawn(async move {
    loop {
        tokio::time::sleep(until_next_midnight()).await;
        copy_db_to_backup_dir(&db_path, &backup_dir).await;
    }
});
```

### 6. Error handling — thiserror + IntoResponse
All errors flow through `AppError`. No `.unwrap()` in handlers. HTTP status codes are explicit, not accidental.

```rust
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Not found")]        NotFound,
    #[error("Forbidden")]        Forbidden,
    #[error("Conflict: {0}")]    Conflict(String),
    #[error("Validation: {0}")]  ValidationError(String),
    #[error(transparent)]        Database(#[from] sqlx::Error),
}
impl IntoResponse for AppError { ... }
```

### 7. WebSocket — unified branch-scoped broadcast
One `ConnectionManager` for the entire server. Events are tagged by type (`ChatMessage`, `InventoryUpdate`, `RefillStatusChange`). Every mutation broadcasts to all connections in the same branch. Clients filter by event type in the HTMX WebSocket extension handler.

```rust
pub struct ConnectionManager {
    connections: DashMap<i64, Vec<mpsc::Sender<WsEvent>>>,
}
```

### 8. Salesforce toggle — SALESFORCE_MODE=mock|live
The `SalesforceClient` is a trait. `MockClient` records calls in memory for tests. `LiveClient` makes real reqwest calls with a 10-second timeout (fail open — Salesforce failure logs but does not 500 the approval endpoint). Toggled via env var. Build with mock until the Connected App is provisioned.

### 9. Dev workflow — Axum standalone
Tera template changes require a full Tauri recompile. For UI development: run Axum standalone with `cargo watch -x 'run --bin inventorix-server'`. Only use `cargo tauri dev` for Tauri-specific integration testing (file dialogs, system tray, window behavior).

### 10. WAL mode + busy_timeout
SQLite WAL mode allows concurrent readers. `busy_timeout = 5000ms` prevents `SQLITE_BUSY` errors under write contention. Both are set in `SqliteConnectOptions` at pool creation — not as PRAGMA statements in migrations.
