# financials

A self-hosted personal finance web app for reconciling bank transactions and tracking net worth over time.

---

## Features

### 1. Transaction Reconciliation

Upload your outgoing transactions (e.g. from a budgeting app or spreadsheet) and your bank's cleared transactions as CSV files. The app auto-matches them by amount, then lets you confirm or reject each proposal. Track which items still need matching and which have been ignored.

**Workflow:**
1. Create a **reconciliation session** (e.g. "July 2026 Checking")
2. Upload **outgoing transactions** (the payments you recorded)
3. Upload **reconciled/bank transactions** (what actually cleared)
4. Run **auto-match** — the app pairs transactions with matching amounts
5. Review proposals: **confirm** correct matches, **reject** false ones, or **link/unlink** manually
6. Ignore small unmatched items (fees, interest) to clear them from view
7. Confirm all remaining proposals in one click

### 2. Wealth Tracking

Log balances across assets, debts, and investments to see how your net worth changes over time.

**Workflow:**
1. Create a **portfolio** (e.g. "Personal", "Joint")
2. Add **wealth items** — accounts, investments, property, debts
3. Log **balances** on any date — the app builds a running grid
4. **Import** historical data from CSV with column mapping
5. **Export** any portfolio to CSV
6. View **insights** — charts showing balance trends over time

### 3. Automated Backups

Snapshots of your SQLite database are uploaded to S3-compatible storage (AWS S3, Backblaze B2, MinIO, etc.) on a configurable schedule. Restore from any snapshot directly through the web UI — no process restart needed.

---

## Quick Start

### Prerequisites

- [Rust](https://rustup.rs/) (edition 2024, tested with 1.96+)
- SQLite (dev headers for compilation: `libsqlite3-dev` on Debian, `sqlite-devel` on Fedora, `sqlite-dev` on Alpine)

### Run the app

```bash
# Clone and enter the repo
git clone <repo-url>
cd financials

# Run with default settings
cargo run
```

The app listens on `http://0.0.0.0:3000` (auto-advances to the next available port up to 3100 if 3000 is taken).

### First-time setup

On first boot with a fresh database, you'll see a **"Set Your Password"** page. Enter a password to create the admin account.

Alternatively, set credentials via environment variables:

```bash
ADMIN_PASSWORD=your-secure-password cargo run
```

After setup, you can change your password from **Settings → Password** at any time.

### Docker

A pre-built image is published on GitHub Container Registry:

```bash
docker run -p 3000:3000 \
  -v $PWD/data:/data \
  -e DATABASE_URL=sqlite:///data/data.db \
  -e DB_PATH=/data/data.db \
  ghcr.io/jlcoulter/financials
```

Or with Docker Compose:

```bash
docker compose up -d
```

Or build locally:

```bash
docker build -t financials .
docker run -p 3000:3000 \
  -v $PWD/data:/data \
  -e DATABASE_URL=sqlite:///data/data.db \
  -e DB_PATH=/data/data.db \
  financials
```

---

## Configuration

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite://data.db` | SQLite connection string |
| `DB_PATH` | `data.db` | Path to the SQLite file (used for backups) |
| `ADMIN_PASSWORD` | — | Plaintext admin password (hashed on first boot). If not set, you'll be prompted to create one on first visit. |
| `ADMIN_PASSWORD_HASH` | — | Pre-hashed bcrypt password (overrides `ADMIN_PASSWORD`). Only used on first boot. |
| `STATIC_DIR` | `src/static` | Directory for static assets |
| `SECURE_COOKIES` | `false` | Set to `true` or `1` to add `Secure` + `SameSite=Lax` flags to the session cookie. Enable when running behind TLS (e.g. reverse proxy with HTTPS). Leave off for local/home-lab HTTP deployments. |
| `RUST_LOG` | `financials=debug` | Log level (e.g. `RUST_LOG=financials=info` for quieter output) |

### Backup configuration

Backups are configured through the web UI at **Settings → Backups**. Supported providers:

- **AWS S3** — standard S3-compatible storage
- **Backblaze B2** — uses S3-compatible API
- **Any S3-compatible store** — MinIO, DigitalOcean Spaces, etc.

Configure the interval (minimum 5 minutes), retention (max snapshots to keep), and credentials. Once enabled, the app automatically creates snapshots on the schedule and prunes old ones.

---

## Architecture

| Layer | Crate |
|---|---|
| HTTP | axum 0.8 + axum-extra 0.10 |
| Templates | maud 0.27 (compile-time HTML via Rust macros) |
| Database | sqlx 0.9 (SQLite) |
| Auth | bcrypt 0.19, signed cookies |
| Static files | tower-http 0.7 (ServeDir) |
| Logging | tracing + tracing-subscriber |
| Errors | anyhow + custom AppError |
| Frontend | HTMX (htmx.min.js) for dynamic interactions |

### Project structure

```
src/
  main.rs        App init, router, graceful shutdown, backup scheduler
  lib.rs         AppState, shared types
  error.rs       AppError enum + IntoResponse
  auth.rs        Signup/login/logout handlers
  cookies.rs     Cookie helpers, LoggedInUser extractor
  layout.rs      HTML layout wrapper (nav, theme)
  pages.rs       All page handlers (dashboard, portfolios, reconcile, settings, insights, backup)
  models/
    user.rs      User DB queries
    portfolio.rs Portfolio + wealth item CRUD, balance logging, CSV import/export
    reconcile.rs Reconciliation session CRUD, transaction matching, auto-match logic
    csv_import.rs CSV column detection, parsing, date format inference
    backup.rs    Backup config, S3/B2 client, snapshot create/list/restore/prune
  utils.rs       parse_dollars, format_cents
  static/
    style.css    Dark theme styles
    htmx.min.js  HTMX for AJAX interactions
migrations/
  0001_init.sql              Users table
  0002_financials.sql        Portfolios, wealth_items, balance_logs
  0004_reconcile.sql         Reconcile sessions + transactions
  0005_backup_config.sql     Backup configuration
  0006_b2_endpoint.sql       B2 endpoint support
  0007_backup_no_user.sql    Backup without user association
  0008_db_instance_id.sql    DB instance tracking for backup isolation
  0009_backup_interval.sql   Configurable backup interval
  0010_backup_max_snapshots.sql  Snapshot retention limit
  0011_add_ignored_column.sql    Ignore flag for transactions
  0013_password_change_required.sql  Password change tracking
```

---

## Development

### Running tests

```bash
cargo test
```

Tests use in-memory SQLite databases with migrations applied per test, so they're fast and isolated.

### Code quality

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### Adding a migration

Create a new file in `migrations/` with a descriptive name (e.g. `0014_my_feature.sql`). sqlx runs all unapplied migrations on startup.

### Debug logging

```bash
RUST_LOG=financials=debug cargo run
```

---

## License

MIT
