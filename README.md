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