# financials

Personal finance app for reconciling transactions and tracking net worth over time.

## First-time setup

On first boot the app creates an admin user with no password set. You'll be prompted to set a password on your first login — any password will work to get in, and you'll be redirected to the password setup page immediately.

Set credentials via environment variables (optional):
- `ADMIN_USERNAME` — defaults to `admin`
- `ADMIN_PASSWORD` — sets an initial password (skips the first-run prompt)
- `ADMIN_PASSWORD_HASH` — a bcrypt hash; overrides `ADMIN_PASSWORD`

If neither `ADMIN_PASSWORD` nor `ADMIN_PASSWORD_HASH` is set, the app starts with no password and the user must set one on first login. The password is persisted in the database and survives restarts.

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

### Password management

On first boot no password is set. Log in with any password and you'll be redirected to the **Set Your Password** page. You can also reach it from the **Password** tab in **Settings** on any authenticated page or via `/change-password`.

The password is stored as a bcrypt hash in the database and persists across restarts. To reset it, set `ADMIN_PASSWORD` or `ADMIN_PASSWORD_HASH` in the environment and delete the database — the app will create a fresh one on next startup.