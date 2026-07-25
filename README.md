# financials

Personal finance app for reconciling transactions and tracking net worth over time.

## What it does

Two core functions:

1. Transaction reconciliation — upload outgoing and reconciled (bank) transactions via CSV, auto-match by amount, manually confirm or reject proposals, and track which items still need matching.
2. Wealth tracking — log balances across assets, debts, and investments to see how your position changes over time. Supports CSV import/export with per-column mapping.

Work in progress.

## Stack

| Layer     | Crate                        |
|-----------|------------------------------|
| HTTP      | axum 0.8 + axum-extra 0.10   |
| Templates | maud 0.27 (axum feature)     |
| Database  | sqlx 0.9 (SQLite)            |
| Auth      | bcrypt 0.19, signed cookies  |
| Static    | tower-http 0.7 (ServeDir)   |
| Logging   | tracing + tracing-subscriber |
| Errors    | anyhow + custom AppError     |

## Structure

```
src/
  main.rs        app init, router, AppState
  error.rs       AppError enum + IntoResponse
  auth.rs        signup/login/logout handlers
  cookies.rs     cookie helpers, LoggedInUser extractor
  layout.rs      HTML layout wrapper
  pages.rs       page handlers (including CSV import/export)
  models/
    user.rs      user DB queries
    portfolio.rs portfolio + wealth item queries + CSV import
    reconcile.rs reconciliation DB queries + auto-match
    csv_import.rs CSV column detection + parsing
  utils.rs       parse_dollars, format_cents
  static/
    style.css
    htmx.min.js
migrations/
  0001_init.sql              users table
  0002_financials.sql        portfolios, wealth_items, balance_logs
  0004_reconcile.sql         reconcile sessions + transactions
```

## Running

```bash
cargo run
# with debug logging
RUST_LOG=rust_web=debug cargo run
```

Listens on `0.0.0.0:3000`.

### Environment variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | `sqlite://data.db` | SQLite connection string |
| `DB_PATH` | `data.db` | Path to the SQLite file (used for backups) |
| `ADMIN_USERNAME` | `admin` | Admin login username |
| `ADMIN_PASSWORD` | — | Plaintext admin password (hashed on first boot). If not set, you'll be prompted to create one on first visit. |
| `ADMIN_PASSWORD_HASH` | — | Pre-hashed bcrypt password (overrides `ADMIN_PASSWORD`). Only used on first boot. |
| `STATIC_DIR` | `src/static` | Directory for static assets |
| `SECURE_COOKIES` | `false` | Set to `true` or `1` to add `Secure` + `SameSite=Lax` flags to the session cookie. Enable when running behind TLS (e.g. reverse proxy with HTTPS). Leave off for local/home-lab HTTP deployments. |

### First-time setup

On first boot with a fresh database:

- If `ADMIN_PASSWORD` or `ADMIN_PASSWORD_HASH` is set, the admin user is created with that password.
- If neither is set, you'll see a **"Set Your Password"** page instead of the login form. Enter a new password to continue.

After initial setup, you can change your password from **Settings → Password**. The password survives restarts — environment variables only seed the initial password.