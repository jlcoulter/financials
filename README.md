# rust-web

A boilerplate web app built with Rust: axum, htmx, maud, sqlx, and bcrypt.

## Stack

| Layer       | Crate                        |
|-------------|------------------------------|
| HTTP        | axum 0.8 + axum-extra 0.10  |
| Templates   | maud 0.27 (axum feature)    |
| Database    | sqlx 0.9 (SQLite)           |
| Auth        | bcrypt 0.19, cookies         |
| Static      | tower-http 0.7 (ServeDir)   |
| Logging     | tracing + tracing-subscriber|
| Errors      | anyhow + custom AppError     |

## Structure

```
src/
  main.rs        app init, router
  error.rs       AppError enum + IntoResponse
  auth.rs        signup/login/logout handlers
  cookies.rs     cookie helpers, LoggedInUser extractor
  layout.rs      HTML layout wrapper
  pages.rs        hello, time, dashboard, 404
  models/
    user.rs      DB query functions
  static/
    style.css
    htmx.min.js
migrations/
  0001_init.sql
```

## Running

```bash
cargo run
# with debug logging
RUST_LOG=rust_web=debug cargo run
```

Listens on `0.0.0.0:3000`.

## Routes

| Method | Path       | Description          |
|--------|------------|----------------------|
| GET    | /          | Home (htmx clock)    |
| GET    | /time      | Time fragment        |
| GET    | /signup    | Signup form          |
| POST   | /signup    | Create account       |
| GET    | /login     | Login form           |
| POST   | /login     | Authenticate         |
| GET    | /dashboard | Protected dashboard  |
| POST   | /logout    | Clear session        |
| ANY    | /*         | 404 fallback         |