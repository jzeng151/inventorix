# Inventorix — Roadmap

> Replace Excel + MS Teams + Salepad with a single Windows desktop app for tile sample inventory management.
> NYC branch pilot first. Multi-branch schema from day 1.

---

## Phase 0 — Pre-Launch (before any code)

Decisions and setup required before development starts. Not features — prerequisites.

- [ ] **Pilot success criteria** — confirm with JG before development starts. Suggested: 30-day pilot, coordinator stops using MS Teams for inventory chat, zero manual Excel updates during pilot, all refill requests go through Inventorix, zero "what's the qty on X?" calls from reps. JG signs off.
- [ ] **Salesforce Connected App provisioning** — submit IT request now. Takes 1-2 weeks. Build with `SALESFORCE_MODE=mock` in the meantime.
- [ ] **NYC branch Excel sheet** — obtain the current master file, verify all 8 columns are present (ITEM NUMBER, COLLECTION, GTS DESCRIPTION, NEW BIN, QTY, Overflow Rack, Order #, Notes).
- [ ] **Chat log visibility policy** — 30-minute conversation with JG and the sales team before pilot launch. Explain what is logged, who can see it, how long records are kept.

---

## Phase 1 — MVP: NYC Pilot (1-week build)

**Definition of done:** A coordinator can import the NYC Excel sheet, track inventory live, chat with sales reps per tile, create and approve refill requests (with Salesforce notification), and export an end-of-day digest. All of this runs as a Windows desktop app on a single machine.

### Day 1 — Foundations (Lane A)
- [x] Rust workspace setup (Axum + Tauri + SQLx + Tera + HTMX)
- [x] sqlx-cli migrations: all 6 tables in one migration file (`20260330_initial.sql`)
- [x] SQLite WAL mode + `busy_timeout = 5000ms` in connection options
- [x] `idx_refill_timer` index on `refill_requests(status, timer_expires_at)`
- [x] `AppState`: `SqlitePool`, `Arc<Mutex<bool>>` import lock, `Arc<ConnectionManager>`
- [x] `AuthUser` extractor (`FromRequestParts`) — compiler-enforced branch isolation
- [x] `AppError` enum (thiserror) implementing `IntoResponse`
- [x] Session middleware (tower-sessions + SQLite session store)
- [x] `DESIGN.md` and `CONTRIBUTING.md` created (dev workflow: Axum standalone mode for template iteration)
- [x] Tauri IPC stubs: `pick_excel_file` (hardcoded path), `open_digest_in_browser` (no-op) — **blocks Lane D**

### Days 2-5 — Feature Lanes (parallel)

**Lane A: Axum Foundation**
- [x] Login / logout routes and Tera templates
- [x] `/health` endpoint (DB connectivity, last timer run, active WS connections)
- [x] Background job: `check_expired_timers` (tokio::spawn loop, every 5 minutes)
- [x] Background job: daily SQLite backup to `backups/` directory

**Lane B: Inventory UI**
- [x] `GET /` — inventory table, branch-scoped, sorted by health (red → amber → green)
- [x] Health summary strip: "12 critical · 34 low · 654 healthy" (clickable filters, multi-select)
- [x] Search/filter: item number, collection, bin, notes (auto-focused on load)
- [x] Color-coded rows (CSS row tint + qty badge with color dot)
- [x] Role-aware column rendering (Refill button hidden for Sales Rep, Approve hidden for Coordinator)
- [x] `GET /tiles/:id` — tile detail: two-column layout (60% fields + 40% chat)
- [x] HTMX partial: row swap on qty update (WebSocket-triggered)
- [x] WebSocket disconnect banner ("Real-time updates paused — reconnecting...")

**Lane C: WebSocket + Real-time**
- [x] `ConnectionManager`: `DashMap<branch_id, Vec<Sender<WsEvent>>>`
- [x] `GET /ws` — WebSocket upgrade handler, branch-scoped via session
- [x] Event types: `ChatMessage`, `InventoryUpdate`, `RefillStatusChange`
- [x] Broadcast on every inventory mutation
- [x] Dead connection cleanup on disconnect (no panic on closed sender)

**Lane D: Excel Import + Export**
- [x] `POST /import` — calamine reads .xlsx, upserts tiles in a single transaction
- [x] Import lock (409 if import already running)
- [x] Import validation: missing columns (422), .xls file (400), non-numeric qty (422), concurrent import (409)
- [ ] Progress feedback via HTMX streaming or polling
- [x] `POST /export` — rust_xlsxwriter generates .xlsx with 8 original columns + QTY_CHANGE + LAST_UPDATED + REFILL_STATUS
- [x] HTML digest: tiles changed since last import, tiles in DB not in import (flagged, not deleted)
- [x] Export is idempotent per calendar day (overwrites same file)
- [ ] Tauri IPC: real `pick_excel_file` (FileDialogBuilder), real `open_digest_in_browser`

**Lane E: Tauri Shell**
- [ ] Tauri app setup, `tokio::spawn` Axum server in setup hook
- [ ] `TcpListener::bind` before spawn (race condition prevention)
- [ ] Tauri IPC stubs (Day 2), real implementations (Day 3-4)
- [ ] `.msi` installer configuration
- [ ] Auto-start on Windows boot (Tauri plugin or Windows startup registry)

**Lane F: Refill Workflow + Salesforce**
- [x] `POST /tiles/:id/refill` — create request, start 48h timer, broadcast to branch
- [x] `POST /refill/:id/approve` — 403 for Coordinator role, triggers Salesforce call
- [x] `POST /refill/:id/fulfill` — Coordinator/Admin only, sets fulfilled_by + fulfilled_at
- [x] `SalesforceClient` trait + `MockClient` (records calls for tests)
- [x] `LiveClient` via reqwest with 10-second timeout (fail open — log error, don't 500)
- [x] `SALESFORCE_MODE=mock|live` env var toggle
- [x] Timer expiry: mark as 'expired', broadcast to branch coordinator

**Admin**
- [x] `GET /admin` — two-panel user list + detail/create form
- [x] `POST /admin/users` — create user with role, branch, email, password
- [x] `POST /admin/users/:id/deactivate` — deactivate + delete all sessions immediately

### Tests (alongside each lane — TDD per lane)
- [x] P0: `GET /tiles/:id` with tile in branch B, user in branch A → 403
- [x] P0: `POST /refill/:id/approve` with Coordinator role → 403
- [x] P0: `GET /tiles` returns only authenticated user's branch tiles
- [x] Auth: valid credentials → session cookie; wrong password → 200 with error (no enumeration)
- [x] Import: valid file, missing column, duplicate item_number (upsert), .xls rejection
- [x] Refill: full state machine (pending → approved → fulfilled, expired path)
- [x] WebSocket: broadcast to branch, dead connection cleanup, branch scoping
- [x] Admin: deactivate invalidates session immediately

---

## Phase 2 — Pilot Hardening (before wider rollout)

Required before adding a second branch or expanding beyond NYC.

- [ ] **Rate limiting on login** — max 5 failed attempts per IP per minute. Returns 429 with `Retry-After` header.
- [ ] **Chat log visibility notice** — "Your conversations in this app are logged for business records purposes." Shown on first login, requires acknowledgement.
- [ ] **Offsite backup** — Windows Task Scheduler or robocopy copies `inventorix.db` + `backups/` to a UNC path or OneDrive nightly. IT configures. Documented recovery procedure.
- [ ] **Data migration tooling** — documented process for importing other branch Excel sheets into the same DB (same import pipeline, different branch_id).

---

## Phase 3 — Post-Pilot (after NYC pilot success criteria met)

Requires JG sign-off on pilot. Each item depends on 30+ days of production data.

- [ ] **Trend analytics dashboard** — qty over time per tile, collection-level trends, 30-day velocity for refill suggestions. `inventory_events` data logged from day 1 — dashboard is ready to build once the data exists.
- [ ] **QR code inventory audit** — generate QR codes linking to `GET /tiles/:id`. Sample coordinators scan with phone or USB scanner for physical audits. Requires physical label printing (coordinate with JG's team, 700 tiles).
- [ ] **PDF digest export** — export end-of-day digest as PDF in addition to HTML. Depends on HTML digest validated by users.
- [ ] **Multi-branch rollout** — expand to additional branches using the same import pipeline. Each branch gets its own data, same UI.
- [ ] **Bidirectional Salesforce sync** — receive order fulfillment status from Salesforce automatically (currently coordinator marks fulfilled manually). Under discussion with Salesforce team. Requires Connected App permissions expansion.

---

## 12-Month Vision

```
CURRENT STATE            PHASE 1 (MVP)           12-MONTH IDEAL
──────────────────────────────────────────────────────────────
Manual Excel         ─→  DB + Excel sync    ─→   Multi-branch DB
Teams chat           ─→  In-app chat log    ─→   Full order comms
Salepad external     ─→  Built in-house     ─→   Full order mgmt
No trend data        ─→  Event logging      ─→   Analytics dashboard
Single branch        ─→  NYC pilot          ─→   All branches
No mobile            ─→  Responsive web     ─→   Mobile-first
Manual Salesforce    ─→  Outbound push      ─→   Bidirectional sync
```

Inventorix becomes the operating layer for every branch's sample workflow. Managers see a live cross-branch dashboard. Every refill request auto-creates a Salesforce task. Smart refill suggestions (from 30-day velocity) prevent overstock and stockouts. The coordinator stops using Excel, Teams, and Salepad entirely.

---

## Not in Scope (any phase)

- Pricing data to warehouse — explicit requirement, never included in Salesforce payloads
- Customer-facing access — internal tool only
- SaaS / multi-tenant external deployment
- Native mobile app — web is mobile-responsive
- SMTP email delivery — digest is HTML file opened in browser, forwarded manually
