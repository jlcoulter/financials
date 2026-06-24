# Financials App — 20-Item Improvement Task List

Idiomatic Rust & Good UX. Prioritized: safety → correctness → polish.

---

## Architecture & Rust Idioms

1. **Extract `format_cents` into a shared module** — Currently duplicated between
   `pages.rs` and `pages_features.rs`. Move to a `src/utils.rs` or `src/format.rs`
   module. Also add a `format_cents_abs` variant that omits the sign, since many
   call sites do `format_cents(val.abs())` with a separate sign prefix.

2. **Replace raw-SQL string queries with `sqlx::query!` or `query_as!` macros**
   — The model files use runtime `sqlx::query(...)` with string literals. The
   `query!` macro validates SQL at compile time against the DB schema, catching
   typos and column-mismatch bugs before the app starts. Requires setting
   `SQLX_OFFLINE=true` and running `cargo sqlx prepare` once, but eliminates
   an entire class of runtime errors. At minimum, convert all `query_as::<_, (String, ...)>`
   row-packing patterns to `query_as!` with named structs.

3. **Derive `sqlx::FromRow` instead of manual row unpacking** — Every model function
   manually unpacks `(String, String, i64, ...)` tuples and calls `Uuid::parse_str`,
   `NaiveDate::parse_from_str`, etc. Define proper structs with `#[derive(FromRow)]`
   and let sqlx handle column mapping. Eliminates ~80 lines of boilerplate across
   `portfolio.rs` and `features.rs`.

4. **Add user_id ownership to transactions, budgets, goals, holidays** — Currently
   these tables have no `user_id` column, so every user sees everyone's data. Add
   a `user_id TEXT REFERENCES users(id)` column and filter all queries by the
   logged-in user. This is a data-safety bug — without it, any logged-in user
   can see and delete other users' data.

5. **Add input validation & server-side bounds checking** — Amounts accept any f64
   including NaN/Inf/negative-for-income. Dates are parsed but not validated for
   reasonableness (e.g., year 0001). Budget month strings aren't validated as
   YYYY-MM. Add a `validate` module with helpers like `validate_amount()`,
   `validate_date()`, `validate_month()` that return `Result` with clear errors.

6. **Wrap `AppState` fields — don't expose `pub key: Key`** — The signing key is
   `pub` for convenience. Use `pub(crate)` or a getter method. Similarly, `db`
   already has a getter; make the field `db: SqlitePool` (not `pub`).

---

## Database & Data Integrity

7. **Add CHECK constraints to the migration** — `amount` should be `CHECK(amount != 0)`
   in transactions, `start_date <= end_date` in holidays, `planned_amount > 0` in
   budgets, `target_amount > 0` in savings_goals. These prevent garbage data at
   the DB level regardless of frontend bugs.

8. **Add created_at/updated_at timestamps to feature tables** — The transactions,
   budgets, savings_goals, and holidays tables lack `created_at`. Add
   `created_at TEXT DEFAULT (datetime('now'))` to each. Useful for sorting and
   auditing. Also add `updated_at` where edits are supported (goals).

9. **Add soft-delete or audit trail** — Currently deletes are hard deletes. At
   minimum, add a `deleted_at TEXT` column and filter `WHERE deleted_at IS NULL`
   in queries. This protects against accidental data loss and enables "undo"
   functionality later.

---

## UX & Frontend

10. **Pagination on transactions list** — With 80+ seed transactions and no limit,
    the page grows unbounded. Add `LIMIT 50 OFFSET ?` with page navigation
    (Prev/Next buttons, page number indicator). Also add a "show all" toggle for
    power users.

11. **Flash messages for success/error after form submissions** — After creating
    a transaction, budget, goal, or holiday, the user gets a redirect but no
    confirmation that it worked. Use cookie-based flash messages (or a query
    param like `?created=1`) to show a green "Created successfully" banner at
    the top of the page.

12. **Inline edit for transaction amounts & descriptions** — Currently you can
    only delete transactions, not edit them. Add HTMX inline editing (like the
    portfolio grid cells) for amount and description on the transactions page.
    Also add an edit page/form as a fallback.

13. **Edit support for budgets, goals, and holidays** — Same issue: you can
    create and delete, but not edit. Budget amounts change, goals get new
    targets, holiday dates shift. Add edit forms (or HTMX inline) for each.

14. **Transaction search & export** — Add a free-text search box for the
    description field on the transactions page. Also add a "Download CSV" button
    that exports the filtered transaction list. Simple UX win for reconciliation
    and tax prep.

15. **Active nav link highlighting** — The nav bar doesn't indicate which page
    you're on. Add an `.active` class to the current page's link. Pass the
    current route (or page name) into `layout()` and conditionally apply the
    class. Makes navigation feel grounded.

16. **Mobile-responsive data tables** — The `.data-table` tables overflow on
    mobile (< 600px). Add `overflow-x: auto` wrappers and horizontal scroll for
    the tables. Also consider a card-based layout on small screens for
    transactions (date / description / amount in a stacked card).

17. **Date range quick-select buttons on Transactions & Stats** — Instead of
    requiring manual date input, add preset buttons: "This Month", "Last Month",
    "This Quarter", "This Year". Reduces friction for the most common filter
    operations.

---

## Performance & Reliability

18. **N+1 query on dashboard** — The dashboard handler loops over portfolios and
    calls `list_wealth_items` + `list_balance_logs` for each one. With 10
    portfolios, that's 20+ queries. Refactor to a single JOIN query or batch
    the fetches. Same pattern in the reconciliation handler.

19. **Lazy-load Chart.js only on the Stats page** — The `<script
    src="chart.js">` tag loads Chart.js (~200KB) on every Stats page visit. Move
    it to a `defer` or load it only when the `/stats` route is hit. Better:
    download the file locally into `src/static/` so the app works offline and
    doesn't depend on a CDN.

20. **Add integration tests for route handlers** — Currently there are zero
    automated tests. Set up `#[cfg(test)]` modules using `axum::test` with an
    in-memory SQLite database. Test at minimum: signup/login flow, portfolio
    CRUD, transaction creation & filtering, budget upsert, and goal update.
    This prevents regressions when refactoring (which items 1-9 will cause).