# TODOS — Inventorix

Items deferred from the CEO review (2026-03-29) or flagged as required pre-work.

---

## P1 — Required before launch

### sqlx-cli migration tooling
**What:** Set up sqlx-cli from the first commit. Create the initial migration from the data schema (all 6 tables). Include `PRAGMA busy_timeout = 5000` in SQLx connect options and `CREATE INDEX idx_refill_timer ON refill_requests(status, timer_expires_at)`.
**Why:** Any schema change without sqlx migrations (adding a column, renaming a field) means dropping and recreating the DB — losing all imported tile data. This is a day-1 risk. busy_timeout is mandatory for WAL mode under concurrent writes. The refill timer index prevents full table scans every 5 minutes.
**How to apply:** `cargo install sqlx-cli`, `sqlx migrate add initial`, write all 6 tables in `migrations/20260330_initial.sql`. All subsequent schema changes go through `sqlx migrate add <name>`. SqliteConnectOptions must include `.journal_mode(SqliteJournalMode::Wal).busy_timeout(Duration::from_secs(5))`.
**Effort:** S (human: 2 hours / CC: 5 min)
**Priority:** P1 — include in first implementation commit (Lane A, EOD Day 1)

---

### Excel import validation spec
**What:** Write a test matrix for all Excel import edge cases before writing the import code.
**Why:** Real-world coordinator Excel files have merged cells, hidden rows, duplicate item numbers, encoding issues, and missing values. The import is probably 30% of the implementation work. Building without a validation spec means discovering edge cases in production.
**Edge cases to test:**
- Missing expected column (ITEM NUMBER, QTY, etc.)
- Merged cells in header or data rows
- Duplicate item_number values
- Empty rows / blank rows between data
- Non-numeric qty values
- Special characters in description fields
- File is not .xlsx (e.g., .xls or .csv renamed)
- File is open in Excel when import runs (locked)
**Effort:** M (human: 1 day / CC: 20 min)
**Priority:** P1 — write spec before building import

---

### Pilot success criteria
**What:** Define measurable success criteria for the NYC pilot before starting development.
**Why:** Without success criteria, the pilot either runs indefinitely or gets cancelled for vague reasons. Internal tools live or die on this.
**Suggested criteria (confirm with JG):**
- Pilot duration: 30 days
- Coordinator stops using MS Teams for inventory-related chat (verified via Teams message volume)
- Zero manual updates to the master Excel sheet during pilot (coordinator uses Inventorix export instead)
- All refill requests go through Inventorix (none via email or Teams)
- Zero "what's the qty on X?" calls/messages from sales reps (they check Inventorix directly)
- Decision maker: JG signs off on pilot success
**Effort:** S (human: 1 hour / CC: N/A — requires JG's input)
**Priority:** P1 — decide before development starts

---

### Tauri IPC stub commands (Lane E → Lane D dependency)
**What:** Lane E (Tauri shell) must ship stub implementations of two Tauri commands by EOD Day 2: `pick_excel_file` (returns a hardcoded path) and `open_digest_in_browser` (no-op). Real implementations land Day 3-4.
**Why:** Lane D (Excel import + export) cannot wire up the file picker or digest opener without these stubs. Blocking Lane D until Lane E ships the real Tauri dialogs adds 2 days of latency to a 5-day timeline.
**How to apply:** Lane E adds both `#[tauri::command]` stubs on Day 2. Lane D codes against the stubs. Lane E replaces with real FileDialogBuilder and opener::open on Day 3-4.
**Effort:** XS (CC: 10 min for stubs)
**Priority:** P1 — Lane E Day 2, or Lane D is blocked

### Dev workflow: Axum standalone mode
**What:** Document in CONTRIBUTING.md that Tera template development should be done with Axum running standalone (not inside Tauri), using cargo-watch for hot-reload. Only use the full Tauri bundle for integration testing.
**Why:** Tauri dev mode has no hot-reload for templates — every change requires a full recompile. For a team learning Rust, this significantly slows UI iteration.
**How to apply:** `cargo watch -x 'run --bin inventorix-server'` for template dev. `cargo tauri dev` only for Tauri-specific feature testing.
**Effort:** XS (document on Day 1 setup)
**Priority:** P1 — Lane A documents in CONTRIBUTING.md on Day 1

### DESIGN.md — design system document
**What:** Extract the design system spec from the CEO plan into a standalone `DESIGN.md` file at repo root.
**Why:** The CEO plan will drift. DESIGN.md is the living reference for all template work. Without it, Lane B and Lane E build visually inconsistent interfaces.
**How to apply:** Copy the UI Design Spec section from the CEO plan into DESIGN.md. Lane B owns keeping it current as templates are built.
**Effort:** XS (CC: 5 min)
**Priority:** P1 — Lane A creates the file on Day 1 alongside migrations

---

## P2 — Required before wider rollout

### Offsite disaster recovery
**What:** Configure backup of the SQLite DB file to a different machine (network share, OneDrive, or SharePoint).
**Why:** Current design backs up to the same local machine daily. Same failure domain. Machine dies = data loss since last backup. For a company running $100M in operations, this is real risk.
**Implementation:** Windows Task Scheduler or robocopy script that copies `inventorix.db` and `backups/` to a UNC path or cloud folder. Runs nightly. IT configures in ~20 minutes.
**Recovery procedure:** (document at launch) Stop NSSM service, restore DB file from backup, restart service.
**Effort:** S (human: 2 hours including IT config)
**Priority:** P2 — configure during NYC rollout, before adding a second branch

---

### Chat log visibility policy
**What:** 30-minute conversation with JG and the sales reps team: explain what gets logged, who can see it, and how long records are kept.
**Why:** Tile-scoped chat is a permanent audit log visible to admins. Sales reps should know this before they start using it — otherwise it's a trust problem at launch.
**Action:** Add a notice on first login ("Your conversations in this app are logged for business records purposes"). Document retention policy (e.g., 1 year, then archiveable).
**Effort:** S (conversation: 30 min; notice in UI: XS)
**Priority:** P2 — before pilot launch

---

### Network drive path for chat logs — confirm with IT
**What:** Replace the placeholder `CHAT_LOG_PATH` env var in `.env.example` with the actual UNC path or mapped drive for the NYC branch network share.
**Why:** Chat logs are written to one file per day (e.g. `2026-03-30.log`) on a network drive, not in SQLite. The implementation uses a placeholder path; IT needs to provide the real path before pilot launch.
**Action:** IT provides the network share path → update `.env.example` and deployment docs.
**Effort:** XS (once IT confirms path)
**Priority:** P2 — before pilot launch

---

### Rate limiting on login endpoint
**What:** Add rate limiting to `POST /auth/login` (max 5 failed attempts per IP per minute).
**Why:** Current security review flagged this as the only unmitigated brute-force vector. The app runs on LAN, reducing risk, but the correct behavior is still to rate-limit.
**Effort:** XS (FastAPI + slowapi, 10-minute add)
**Priority:** P2

---

## Phase 2 — Post-pilot

### QR code inventory audit
**What:** Generate QR codes for each tile record. Sample coordinators scan tile QR codes (phone camera or USB scanner) for physical inventory audits.
**Why:** Removes lookup friction when physically checking bins. Audit trail per scan.
**Implementation notes:** QR = URL to `GET /tiles/{item_id}`. Item IDs stable from day 1. QR generation trivial with `qrcode` library.
**Depends on:** Physical label printing for 700 tiles (coordinate with JG's team).
**Effort:** M (QR generation: S; label printing logistics: M)

---

### Trend analytics dashboard
**What:** UI showing sales velocity, stock levels over time, and suggested reorder quantities.
**Why:** `inventory_events` table logs every qty change from day 1. After 30+ days of data, the dashboard becomes immediately useful.
**Implementation notes:** Charts on qty over time per tile, collection-level trends, 30-day velocity for refill suggestions.
**Depends on:** 30+ days of production inventory data.
**Effort:** L (human: 1 week / CC: 1 hour)

---

### Bidirectional Salesforce sync
**What:** Receive order fulfillment status from Salesforce back into Inventorix. When warehouse marks a refill order shipped, Inventorix refill_request status updates to fulfilled automatically.
**Why:** Currently the coordinator manually marks fulfilled. Bidirectional sync closes the loop.
**Status:** Under discussion with Salesforce team. Requires Connected App permissions expansion.
**Effort:** M-L

---

### PDF digest export
**What:** Export the end-of-day digest as a PDF (in addition to HTML).
**Why:** PDF is more shareable and printable for managers.
**Depends on:** HTML digest shipped and validated with users.
**Effort:** S (WeasyPrint or similar)

---

### Data migration from other branches
**What:** Import Excel sheets from branches beyond NYC into Inventorix.
**Why:** Multi-branch expansion after NYC pilot proves value.
**Depends on:** Pilot success criteria met; management approval for rollout.
**Effort:** S per branch (same import pipeline)
