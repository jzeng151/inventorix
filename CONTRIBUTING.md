# Contributing to Inventorix

## Setup

```bash
# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Install sqlx-cli
cargo install sqlx-cli --no-default-features --features sqlite

# Install cargo-watch (for template hot-reload)
cargo install cargo-watch

# Install Tauri CLI
cargo install tauri-cli

# Copy environment file
cp .env.example .env
# Edit .env — set DB_PATH and SESSION_SECRET at minimum
# Leave SALESFORCE_MODE=mock for local dev

# Run migrations
sqlx migrate run

# Start the server (template dev mode — no Tauri, fast reload)
cargo watch -x 'run --bin inventorix-server'
# Open http://localhost:3000
```

## Dev Workflow

### Template iteration
Run Axum standalone with cargo-watch. No Tauri recompile on template changes.

```bash
cargo watch -x 'run --bin inventorix-server'
```

Tera templates in `templates/` reload on file change. CSS in `static/css/` serves directly.

### Full Tauri bundle
Only needed when testing Tauri-specific behavior: file dialogs, system tray, window management, `.msi` installer.

```bash
cargo tauri dev
```

### Tests
```bash
cargo test                    # all tests
cargo test auth               # specific module
cargo test -- --nocapture     # show println output
```

Tests use `#[sqlx::test]` — each test gets a fresh in-memory SQLite DB with migrations auto-applied. No shared state between tests.

### Migrations

```bash
# Apply pending migrations
sqlx migrate run

# Create a new migration
sqlx migrate add <descriptive_name>
# Edit the generated file in migrations/
# Then: sqlx migrate run
```

Never modify an existing migration file after it has been committed. Create a new one.

### Adding a new route

1. Add the handler in `src/routes/<module>.rs`
2. Accept `AuthUser` as a parameter (compiler error if you forget)
3. Scope all DB queries to `auth.branch_id` (unless admin bypass)
4. Add the route to the router in `src/routes/mod.rs`
5. Write the `#[sqlx::test]` test alongside the handler

### Adding a new Tera template

1. Create the template in `templates/`
2. Use CSS custom properties from `static/css/app.css` — never hardcode colors
3. Use role-aware rendering (different HTML per role, not `display:none`)
4. Include ARIA labels on all color-coded elements
5. Test with `cargo watch -x 'run --bin inventorix-server'`, not Tauri

## Lane Assignments

See `CLAUDE.md` for the full lane structure. Lane A ships EOD Day 1 and everything else starts Day 2.

**Lane E note:** Ship `pick_excel_file` and `open_digest_in_browser` Tauri IPC stubs by EOD Day 2 — Lane D (Excel import/export) is blocked without them.

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `DB_PATH` | Yes | Path to SQLite file (e.g., `./inventorix.db`) |
| `SESSION_SECRET` | Yes | 32+ byte random string for session signing |
| `SALESFORCE_MODE` | Yes | `mock` (local dev, tests) or `live` (production) |
| `SALESFORCE_CLIENT_ID` | If live | Salesforce Connected App client ID |
| `SALESFORCE_CLIENT_SECRET` | If live | Salesforce Connected App client secret |
| `SALESFORCE_INSTANCE_URL` | If live | e.g., `https://company.my.salesforce.com` |
| `BACKUP_DIR` | No | Path for daily SQLite backups (default: `./backups`) |
| `PORT` | No | HTTP port (default: 3000) |

Always use `SALESFORCE_MODE=mock` for local development and tests. Never commit `.env`.

## Code Style

- No `.unwrap()` in handlers. Use `?` and `AppError`.
- No raw `sqlx::query(...)`. Use `sqlx::query!` macros (compile-time verification).
- No hardcoded hex colors in templates. Use CSS custom properties.
- One migration file per schema change. No modifying committed migrations.
- Test file per route module in `tests/`.

See `CLAUDE.md` for the full list of patterns and anti-patterns.
