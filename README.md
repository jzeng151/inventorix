# Inventorix

Internal inventory management system for a tile sample warehouse. Replaces a spreadsheet + chat workflow with a single Windows desktop app and companion Android scanner.

---

## Features

### Inventory Management
- Live inventory table with color-coded health indicators (critical / low / healthy) based on configurable quantity thresholds
- Per-item tile cards with give-out tracking, quantity correction, and notes
- Column sorting and multi-field search (item number, collection, bin, notes)
- Excel import (`.xlsx`) with upsert logic — new items created, existing records updated
- End-of-day Excel export with an HTML digest automatically opened in the browser

### Restock Workflow
- Sales reps request restocks from the tile detail card
- Admins approve or deny requests; denials require a written reason
- Coordinators confirm fulfilled restocks via QR scan or manual entry
- 48-hour approval timer with automatic expiry; Salesforce notified on approval
- Pending-fulfillment badges visible across the inventory, history, and analytics views

### QR Code Scanner (Android)
- Companion Android app connects to the desktop server over office WiFi
- Two scanning modes selected at app launch and held until manually exited:
  - **Confirm Restock** — scan a shelved item to mark its approved restock as fulfilled
  - **Verify Inventory** — scan an item, view the database quantity, and confirm or correct it on the spot
- Full audit trail written to `scan_audits` for every scan
- Printable QR label sheet (`/scan/print-qr`) — bulk-generate SVG codes for all items

### History
- Unified action log: give-outs, restock requests, approvals, rejections, fulfillments, inventory scans, corrections, and note edits
- Filterable by item number, person, date, action type, and pending-fulfillment status
- Processing time shown on each restock entry (requested → approved/rejected)
- Rejection reasons displayed inline

### Analytics
- Date-range summary cards: total give-outs, restock requests, quantity restocked, rejections, pending fulfillment, inventory scans, corrections
- Threshold controls: highlight items exceeding a give-out or restock count
- Top-20 bar charts (give-outs by item, restock requests by item) with flagged items highlighted in red
- Flagged tiles dialog and pending-only filter
- Item search with live table filtering

### Real-Time Collaboration
- Branch-scoped chat sidebar on the inventory page (WebSocket broadcast)
- Chat messages link detected item numbers directly to the tile card
- Admins are notified of new restock requests in real time
- Coordinators are notified when a restock is approved
- Inventory quantity updates pushed instantly to all connected sessions

### Admin
- User management: create, deactivate, and reset passwords for branch users
- Session invalidation on deactivate — user is signed out immediately
- Role-based access enforcement at the API level (admin / coordinator / sales rep)
- Branch isolation: all queries are scoped to the authenticated user's branch

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Desktop shell | Tauri 2 (Rust) — single `.msi` installer |
| Mobile shell | Tauri 2 Android — APK connecting to desktop server over LAN |
| Web server | Axum, spawned inside the Tauri process |
| Database | SQLite (WAL mode) via SQLx with compile-time query verification |
| Templates | Tera (server-rendered) + HTMX for partial swaps |
| Real-time | WebSocket branch-scoped broadcast |
| Auth | tower-sessions + SQLite session store; argon2 password hashing |
| Excel | calamine (import) + rust_xlsxwriter (export) |
| QR codes | `qrcode` crate — SVG generation server-side |
| Salesforce | reqwest (rustls-tls) via a mockable trait |

---

## Roles

| Role | Capabilities |
|------|-------------|
| Admin | Full access — user management, approve/deny restocks, all data |
| Coordinator | Inventory management, give-outs, Excel import/export, QR scanning |
| Sales Rep | View inventory, request restocks, branch chat |

---

## Development

```bash
# Run the server standalone (fast template iteration)
cargo run --bin inventorix-server
# → http://localhost:3000

# Full Tauri desktop shell
cargo tauri dev

# Apply migrations
sqlx migrate run

# Build Windows MSI (run on Windows)
cargo tauri build

# Build Android APK
cargo tauri android build --apk
```

Copy `.env.example` to `.env` and set `SALESFORCE_MODE=mock` for local development.
