use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::flash::Flash;
use crate::layout::{layout, active, active_flash};
use crate::models::features;
use crate::models::portfolio::{self, BalanceLog, WealthItem};
use crate::utils;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use chrono::NaiveDate;
use std::collections::BTreeMap;

// ── Helpers ──

fn format_cents(cents: i64) -> String {
    utils::format_cents(cents)
}

fn format_dollars(cents: i64) -> String {
    utils::format_dollars(cents)
}

/// Pivot flat balance_logs into grid rows keyed by date.
struct GridRow {
    date: NaiveDate,
    values: Vec<Option<i64>>,
}

fn pivot_logs(items: &[WealthItem], logs: &[BalanceLog]) -> Vec<GridRow> {
    let item_index: std::collections::HashMap<uuid::Uuid, usize> = items
        .iter()
        .enumerate()
        .map(|(i, wi)| (wi.item_id, i))
        .collect();

    let mut by_date: BTreeMap<NaiveDate, Vec<Option<i64>>> = BTreeMap::new();
    for log in logs {
        let row = by_date
            .entry(log.log_date)
            .or_insert_with(|| vec![None; items.len()]);
        if let Some(&idx) = item_index.get(&log.item_id) {
            row[idx] = Some(log.balance_value);
        }
    }

    by_date
        .into_iter()
        .rev()
        .map(|(date, values)| GridRow { date, values })
        .collect()
}

// ── Pages ──

pub async fn hello(
    State(_state): State<AppState>,
    user: Option<LoggedInUser>,
) -> impl IntoResponse {
    if user.is_some() {
        // Redirect logged-in users to dashboard
        return axum::response::Redirect::to("/dashboard").into_response();
    }
    layout(
        "Home",
        maud::html! {
            div class="hero" {
                h1 { "Track Your Wealth" }
                p class="hero-sub" { "Simple portfolio tracking. See the big picture." }
                div class="hero-actions" {
                    a href="/login" class="btn btn-primary" { "Login" }
                    a href="/signup" class="btn" { "Sign up" }
                }
            }
        },
        None,
        false,
    )
    .into_response()
}

pub async fn portfolios(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios_list = portfolio::list_portfolios(state.db(), &user.0).await?;
    // Batch fetch for net worth per portfolio
    let all_items = portfolio::list_all_wealth_items_for_user(state.db(), &user.0).await?;
    let all_logs = portfolio::list_all_balance_logs_for_user(state.db(), &user.0).await?;

    let mut latest_per_item: std::collections::HashMap<uuid::Uuid, (chrono::NaiveDate, i64)> = std::collections::HashMap::new();
    for l in &all_logs {
        let entry = latest_per_item.entry(l.item_id).or_insert((l.log_date, l.balance_value));
        if l.log_date > entry.0 { *entry = (l.log_date, l.balance_value); }
    }

    Ok(active(
        "Portfolios",
        maud::html! {
            div class="page-header" {
                h2 { "Portfolios" }
                a href="/portfolios/new" class="btn btn-primary" { "+ New Portfolio" }
            }
            @if portfolios_list.is_empty() {
                div class="empty-state" {
                    p { "No portfolios yet. Create one to get started." }
                    a href="/portfolios/new" class="btn btn-primary" { "Create Your First Portfolio" }
                }
            } @else {
                div class="portfolio-list" {
                    @for (id, name) in &portfolios_list {
                        @let items_in: Vec<_> = all_items.iter().filter(|i| i.portfolio_id == *id).collect();
                        @let net: i64 = items_in.iter().map(|item| {
                            let val = latest_per_item.get(&item.item_id).map(|(_, v)| *v).unwrap_or(0);
                            if item.item_type == "debt" { -val } else { val }
                        }).sum();
                        @let net_cls = if net >= 0 { "positive" } else { "negative" };
                        div class="portfolio-row" {
                            a href=(format!("/portfolio/{}", id)) class="portfolio-info" {
                                h3 { (name) }
                                span class=(format!("portfolio-meta {}", net_cls)) { (format_cents(net)) " · " (items_in.len()) " items" }
                            }
                            form method="post" action=(format!("/portfolio/{}/delete", id))
                                  class="inline-form"
                                  onsubmit="return confirm('Delete this portfolio and all its data?')" {
                                button type="submit" class="btn btn-danger btn-sm" { "Delete" }
                            }
                        }
                    }
                }
            }
        },
        Some(&user),
        false,
        "portfolios",
    ))
}

// ── New Portfolio page ──

pub async fn new_portfolio_form(
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    Ok(active(
        "New Portfolio",
        maud::html! {
            div class="page-header" {
                a href="/portfolios" class="back-link" { "← Portfolios" }
                h2 { "Create Portfolio" }
            }
            div class="content-card" {
                form method="post" action="/portfolios" class="form-grid" {
                    label { "Portfolio Name"
                        input type="text" name="name" placeholder="e.g. Retirement, Family Finances" required {}
                    }
                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" { "Create Portfolio" }
                        a href="/portfolios" class="btn" { "Cancel" }
                    }
                }
            }
        },
        Some(&user),
        false,
        "portfolios",
    ))
}

// ── POST: create portfolio ──

#[derive(serde::Deserialize)]
pub struct CreatePortfolioForm {
    name: String,
}

pub async fn create_portfolio(
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<CreatePortfolioForm>,
) -> Result<axum::response::Redirect, AppError> {
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Portfolio name is required".into()));
    }
    portfolio::create_portfolio(state.db(), form.name.trim(), &user.0).await?;
    Ok(axum::response::Redirect::to("/portfolios"))
}

#[derive(serde::Deserialize, Default)]
pub struct PortfolioFilter {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

pub async fn portfolio(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(filter): axum::extract::Query<PortfolioFilter>,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let grid_rows = pivot_logs(&items, &logs);

    // Current value = latest value per item (most recent date for each item)
    let mut latest_per_item: std::collections::HashMap<uuid::Uuid, (NaiveDate, i64)> = std::collections::HashMap::new();
    for l in &logs {
        let entry = latest_per_item.entry(l.item_id).or_insert((l.log_date, l.balance_value));
        if l.log_date > entry.0 {
            *entry = (l.log_date, l.balance_value);
        }
    }
    let current_net: i64 = latest_per_item.iter()
        .map(|(id, (_, val))| {
            let item = items.iter().find(|i| i.item_id == *id);
            match item.map(|i| i.item_type.as_str()) {
                Some("debt") => -val,
                _ => *val,
            }
        })
        .sum();

    // Item chips show current (latest) value
    let item_current: Vec<(String, String, i64)> = items.iter().map(|item| {
        let current = latest_per_item.get(&item.item_id)
            .map(|(_, v)| *v)
            .unwrap_or(0);
        (item.name.clone(), item.item_type.clone(), current)
    }).collect();

    Ok(active_flash(
        &format!("Portfolio - {}", name),
        maud::html! {
            div class="page-header" {
                a href="/portfolios" class="back-link" { "← Portfolios" }
                h2 { (name) }
                a href=(format!("/portfolio/{}/import", portfolio_id)) class="btn btn-sm" { "Import CSV" }
                a href=(format!("/portfolio/{}/export/csv", portfolio_id)) class="btn btn-sm" { "Export CSV" }
                form method="post" action=(format!("/portfolio/{}/delete", portfolio_id))
                      class="inline-form"
                      onsubmit="return confirm('Delete this portfolio and all its data?')" {
                    button type="submit" class="btn btn-danger btn-sm" { "Delete Portfolio" }
                }
            }

            div class="summary-cards" {
                div class="summary-card" {
                    span class="summary-label" { "Current Value" }
                    span class="summary-value" { (format_cents(current_net)) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Items" }
                    span class="summary-value" { (items.len()) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Entries" }
                    span class="summary-value" { (grid_rows.len()) }
                }
            }

            // Add wealth item
            details class="add-section" {
                summary { "+ Add Wealth Item" }
                form method="post" action=(format!("/portfolio/{}/items", portfolio_id)) {
                    label { "Name" input type="text" name="name" required {} }
                    label { "Type"
                        select name="item_type" {
                            option value="asset" { "Asset" }
                            option value="debt" { "Debt" }
                            option value="investment" { "Investment" }
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Add Item" }
                }
            }

            // Add balance row
            details class="add-section" {
                summary { "+ Add Balance Row" }
                form method="post" action=(format!("/portfolio/{}/balances", portfolio_id)) {
                    label { "Date" input type="date" name="log_date" required {} }
                    @for item in &items {
                        label { (item.name)
                            input type="number" step="0.01"
                                name=(format!("balance_{}", item.item_id))
                                placeholder="$0.00" {}
                        }
                    }
                    button type="submit" class="btn btn-primary" { "Save Row" }
                }
            }

            // Item current values
            @if !item_current.is_empty() {
                div class="item-totals" {
                    h3 { "Current Values" }
                    div class="item-totals-grid" {
                        @for (iname, itype, val) in &item_current {
                            @let iid = items.iter().find(|i| i.name == *iname).unwrap().item_id;
                            div class=(format!("item-chip item-chip-{}", itype)) {
                                span class="item-chip-name" { (iname) }
                                span class="item-chip-value" { (format_cents(*val)) }
                                form method="post" action=(format!("/portfolio/{}/items/delete", portfolio_id))
                                      class="inline-form"
                                      onsubmit="return confirm('Delete this item and all its data?')" {
                                    input type="hidden" name="item_id" value=(iid) {}
                                    button type="submit" class="btn btn-danger btn-xs" { "×" }
                                }
                            }
                        }
                    }
                }
            }

            // Grid table
            @if !items.is_empty() {
                div class="grid-wrapper" {
                    table {
                        thead {
                            tr {
                                th { "Date" }
                                @for item in &items {
                                    th { (item.name) }
                                }
                            }
                        }
                        tbody id="grid-body" {
                            @for row in &grid_rows {
                                tr {
                                    td class="date-cell" {
                                        span { (row.date) }
                                        form method="post" action=(format!("/portfolio/{}/balances/delete", portfolio_id))
                                              class="inline-form row-delete"
                                              onsubmit="return confirm('Delete this entire row?')" {
                                            input type="hidden" name="log_date" value=(row.date.format("%Y-%m-%d").to_string()) {}
                                            button type="submit" class="btn btn-danger btn-xs" { "×" }
                                        }
                                    }
                                    @for (idx, val) in row.values.iter().enumerate() {
                                        @let item_id = items[idx].item_id;
                                        @let cell_id = format!("cell-{}-{}", item_id, row.date);
                                        @match val {
                                            Some(cents) => {
                                                td id=(cell_id)
                                                   class="editable"
                                                   tabindex="0"
                                                   hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, row.date))
                                                   hx-target=(format!("#{}", cell_id))
                                                   hx-swap="innerHTML" {
                                                    (format_cents(*cents))
                                                }
                                            }
                                            None => {
                                                td id=(cell_id)
                                                   class="editable empty"
                                                   tabindex="0"
                                                   hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, row.date))
                                                   hx-target=(format!("#{}", cell_id))
                                                   hx-swap="innerHTML" {
                                                    "\u{2014}"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            } @else {
                div class="empty-state" {
                    p { "No wealth items yet. Add one above to start tracking." }
                }
            }
        },
        Some(&user),
        true,
        "portfolios",
        &Flash { flash: filter.flash.clone(), flash_type: filter.flash_type.clone() },
    ))
}

// ── Inline cell edit (HTMX partial) ──

#[derive(serde::Deserialize)]
pub struct CellQuery {
    item_id: String,
    date: String,
}

/// GET: return an inline form to edit one cell.
pub async fn edit_cell(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<CellQuery>,
) -> Result<maud::Markup, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let item_id = uuid::Uuid::parse_str(&query.item_id)
        .map_err(|e| AppError::BadRequest(format!("Invalid item_id: {}", e)))?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid date: {}", e)))?;

    // Find current value (if any)
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let current_cents = logs.iter()
        .find(|l| l.item_id == item_id && l.log_date == date)
        .map(|l| l.balance_value);

    let cell_id = format!("cell-{}-{}", item_id, date);
    let display_val = current_cents.map(|c| format_dollars(c)).unwrap_or_default();

    let cancel_url = format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date);
    let target_sel = format!("#{}", cell_id);

    Ok(maud::html! {
        form class="cell-edit-form"
              hx-put=(format!("/portfolio/{}/cell", portfolio_id))
              hx-target=(target_sel.clone())
              hx-swap="innerHTML"
              hx-trigger="submit" {
            input type="hidden" name="item_id" value=(item_id) {}
            input type="hidden" name="date" value=(date) {}
            input type="number" step="0.01" name="value"
                   value=(display_val)
                   class="cell-edit-input"
                   hx-on--blur="this.closest('form').requestSubmit()"
                   hx-on--keydown=(format!("if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','{}',{{target:'{}',swap:'innerHTML'}})}}", cancel_url, target_sel))
                   autofocus {}
        }
    })
}

/// PUT: save the edited cell value, return the formatted display.
pub async fn save_cell(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let item_id_str = form.get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = uuid::Uuid::parse_str(item_id_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid item_id: {}", e)))?;
    let date_str = form.get("date")
        .ok_or_else(|| AppError::BadRequest("Missing date".into()))?;
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid date: {}", e)))?;
    let value_str = form.get("value")
        .ok_or_else(|| AppError::BadRequest("Missing value".into()))?;

    let cell_id = format!("cell-{}-{}", item_id, date);

    // Empty value = delete? For now treat empty as 0 or skip.
    if value_str.trim().is_empty() {
        // Return the empty cell
        return Ok(maud::html! {
            td id=(cell_id)
               class="editable empty"
               tabindex="0"
               hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
               hx-target=(format!("#{}", cell_id))
               hx-swap="innerHTML" {
                "\u{2014}"
            }
        });
    }

    let dollars: f64 = value_str.parse()
        .map_err(|e| AppError::BadRequest(format!("Invalid number: {}", e)))?;
    let cents = (dollars * 100.0).round() as i64;

    portfolio::upsert_balance_log(state.db(), item_id, date, cents).await?;

    Ok(maud::html! {
        td id=(cell_id)
           class="editable"
           tabindex="0"
           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
           hx-target=(format!("#{}", cell_id))
           hx-swap="innerHTML" {
            (format_cents(cents))
        }
    })
}

// ── POST: add wealth item ──

#[derive(serde::Deserialize)]
pub struct AddItemForm {
    name: String,
    item_type: String,
}

pub async fn add_item(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<AddItemForm>,
) -> Result<axum::response::Redirect, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    portfolio::create_wealth_item(state.db(), portfolio_id, &form.name, &form.item_type).await?;
    Ok(axum::response::Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

// ── POST: add balance row ──
// Use HashMap because field names are dynamic (balance_{uuid}).

pub async fn add_balance(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Redirect, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let log_date_str = form
        .get("log_date")
        .ok_or_else(|| AppError::BadRequest("Missing log_date".into()))?;
    let log_date = NaiveDate::parse_from_str(log_date_str, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid date: {}", e)))?;

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;

    for item in &items {
        let key = format!("balance_{}", item.item_id);
        if let Some(value) = form.get(&key) {
            if let Ok(cents) = utils::parse_dollars(value) {
                portfolio::insert_balance_log(state.db(), item.item_id, log_date, cents).await?;
            }
            // Empty or non-numeric = skip that cell
        }
    }

    Ok(axum::response::Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

// ── POST: delete portfolio ──

pub async fn delete_portfolio(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::delete_portfolio(state.db(), portfolio_id, &user.0).await?;
    Ok(axum::response::Redirect::to("/portfolios"))
}

// ── POST: delete wealth item ──

pub async fn delete_item(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Redirect, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let item_id_str = form.get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = uuid::Uuid::parse_str(item_id_str)
        .map_err(|e| AppError::BadRequest(format!("Invalid item_id: {}", e)))?;
    portfolio::delete_wealth_item(state.db(), item_id).await?;
    Ok(axum::response::Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

// ── POST: delete balance row (all entries for a date) ──

#[derive(serde::Deserialize)]
pub struct DeleteRowForm {
    log_date: String,
}

pub async fn delete_balance_row(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<DeleteRowForm>,
) -> Result<axum::response::Redirect, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let log_date = NaiveDate::parse_from_str(&form.log_date, "%Y-%m-%d")
        .map_err(|e| AppError::BadRequest(format!("Invalid date: {}", e)))?;
    portfolio::delete_balance_row(state.db(), portfolio_id, log_date).await?;
    Ok(axum::response::Redirect::to(&format!("/portfolio/{}", portfolio_id)))
}

// ── Stats / Charts page ──

#[derive(serde::Deserialize, Default)]
pub struct StatsFilter {
    pub portfolio: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
}

pub async fn stats(
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(filter): axum::extract::Query<StatsFilter>,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(state.db(), &user.0).await?;

    // Parse filter params
    let filter_portfolio: Option<uuid::Uuid> = filter.portfolio
        .as_deref()
        .and_then(|s| uuid::Uuid::parse_str(s).ok());
    let filter_from: Option<NaiveDate> = filter.from
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let filter_to: Option<NaiveDate> = filter.to
        .as_deref()
        .and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    // Collect data across portfolios (filtered) — batch fetch to avoid N+1 queries
    let all_items_batch = portfolio::list_all_wealth_items_for_user(state.db(), &user.0).await?;
    let all_logs_batch = portfolio::list_all_balance_logs_for_user(state.db(), &user.0).await?;

    let mut all_dates_set: std::collections::BTreeSet<NaiveDate> = std::collections::BTreeSet::new();
    // item_id -> (item_name, item_type, portfolio_name, date -> value)
    let mut all_item_logs: std::collections::HashMap<uuid::Uuid, (String, String, String, BTreeMap<NaiveDate, i64>)> = std::collections::HashMap::new();

    let mut portfolio_summaries: Vec<(uuid::Uuid, String, i64, i64, i64, i64, usize)> = Vec::new();
    // type_totals: type -> signed value (debt is negative)
    let mut type_totals: std::collections::HashMap<String, i64> = std::collections::HashMap::new();

    for (pid, pname) in &portfolios {
        if let Some(fp) = filter_portfolio {
            if *pid != fp { continue; }
        }
        let items: Vec<_> = all_items_batch.iter().filter(|i| i.portfolio_id == *pid).collect();
        let logs: Vec<_> = all_logs_batch.iter().filter(|l| items.iter().any(|i| i.item_id == l.item_id)).collect();

        for item in &items {
            let mut series: BTreeMap<NaiveDate, i64> = BTreeMap::new();
            for l in logs.iter().filter(|l| l.item_id == item.item_id) {
                // Apply date filter
                if let Some(from) = filter_from {
                    if l.log_date < from { continue; }
                }
                if let Some(to) = filter_to {
                    if l.log_date > to { continue; }
                }
                series.insert(l.log_date, l.balance_value);
                all_dates_set.insert(l.log_date);
            }
            all_item_logs.insert(item.item_id, (item.name.clone(), item.item_type.clone(), pname.clone(), series));
        }

        // Current net worth (latest value per item, unfiltered — always show current)
        let mut latest_per_item: std::collections::HashMap<uuid::Uuid, (NaiveDate, i64)> = std::collections::HashMap::new();
        for l in &logs {
            let entry = latest_per_item.entry(l.item_id).or_insert((l.log_date, l.balance_value));
            if l.log_date > entry.0 {
                *entry = (l.log_date, l.balance_value);
            }
        }

        let current_net: i64 = latest_per_item.iter()
            .map(|(id, (_, val))| {
                let item = items.iter().find(|i| i.item_id == *id);
                match item.map(|i| i.item_type.as_str()) {
                    Some("debt") => -val,
                    _ => *val,
                }
            })
            .sum();
        let total_assets: i64 = latest_per_item.iter()
            .filter(|(id, _)| items.iter().find(|i| i.item_id == **id).map(|i| i.item_type.as_str()) == Some("asset"))
            .map(|(_, (_, v))| *v)
            .sum();
        let total_investments: i64 = latest_per_item.iter()
            .filter(|(id, _)| items.iter().find(|i| i.item_id == **id).map(|i| i.item_type.as_str()) == Some("investment"))
            .map(|(_, (_, v))| *v)
            .sum();
        let total_debts: i64 = latest_per_item.iter()
            .filter(|(id, _)| items.iter().find(|i| i.item_id == **id).map(|i| i.item_type.as_str()) == Some("debt"))
            .map(|(_, (_, v))| *v)
            .sum();

        for (item_id, (_, val)) in &latest_per_item {
            let item = items.iter().find(|i| i.item_id == *item_id);
            if let Some(item) = item {
                let entry = type_totals.entry(item.item_type.clone()).or_insert(0i64);
                match item.item_type.as_str() {
                    "debt" => *entry -= val,
                    _ => *entry += val,
                }
            }
        }

        portfolio_summaries.push((*pid, pname.clone(), current_net, total_assets, total_investments, total_debts, items.len()));
    }

    let all_dates: Vec<NaiveDate> = all_dates_set.iter().cloned().collect();

    // ── Net worth carry-forward series ──
    let mut last_values: std::collections::HashMap<uuid::Uuid, i64> = std::collections::HashMap::new();
    let mut net_worth_series: Vec<i64> = Vec::new();
    // Per-type carry-forward
    let mut assets_series: Vec<i64> = Vec::new();
    let mut investments_series: Vec<i64> = Vec::new();
    let mut debts_series: Vec<i64> = Vec::new();

    for date in &all_dates {
        for (item_id, (_name, _item_type, _pname, series)) in &all_item_logs {
            if let Some(val) = series.get(date) {
                last_values.insert(*item_id, *val);
            }
        }
        let mut net: i64 = 0;
        let mut assets: i64 = 0;
        let mut investments: i64 = 0;
        let mut debts: i64 = 0;
        for (item_id, val) in &last_values {
            let item_type = all_item_logs.get(item_id).map(|(_, t, ..)| t.as_str()).unwrap_or("asset");
            match item_type {
                "asset" => { assets += val; net += val; }
                "investment" => { investments += val; net += val; }
                "debt" => { debts += val; net -= val; }
                _ => { net += val; }
            }
        }
        net_worth_series.push(net);
        assets_series.push(assets);
        investments_series.push(investments);
        debts_series.push(debts);
    }

    let chart_labels: Vec<String> = all_dates.iter().map(|d| d.format("%Y-%m-%d").to_string()).collect();

    // ── MoM change series ──
    let mut mom_change: Vec<i64> = Vec::new();
    for i in 1..net_worth_series.len() {
        mom_change.push(net_worth_series[i] - net_worth_series[i - 1]);
    }
    let mom_labels: Vec<String> = all_dates.iter().skip(1).map(|d| d.format("%Y-%m").to_string()).collect();

    // ── Per-item trend series (top-level items for multi-line chart) ──
    // Build carry-forward per item
    let mut item_series: Vec<(String, String, Vec<(String, i64)>)> = Vec::new(); // (name, type, [(date_label, value)])
    let mut item_last: std::collections::HashMap<uuid::Uuid, i64> = std::collections::HashMap::new();
    for (item_id, (name, item_type, _pname, series)) in &all_item_logs {
        let mut pts: Vec<(String, i64)> = Vec::new();
        for date in &all_dates {
            if let Some(val) = series.get(date) {
                item_last.insert(*item_id, *val);
            }
            if let Some(&val) = item_last.get(item_id) {
                let signed = match item_type.as_str() {
                    "debt" => -val,
                    _ => val,
                };
                pts.push((date.format("%Y-%m-%d").to_string(), signed));
            }
        }
        if !pts.is_empty() {
            item_series.push((name.clone(), item_type.clone(), pts));
        }
    }

    // Type totals for pie chart
    let type_labels: Vec<String> = type_totals.keys().cloned().collect();
    let type_values: Vec<i64> = type_totals.values().copied().collect();

    // Overall totals
    let total_net_worth: i64 = portfolio_summaries.iter().map(|(_, _, nw, _, _, _, _)| nw).sum();
    let total_assets: i64 = portfolio_summaries.iter().map(|(_, _, _, a, _, _, _)| a).sum();
    let total_investments: i64 = portfolio_summaries.iter().map(|(_, _, _, _, inv, _, _)| inv).sum();
    let total_debts: i64 = portfolio_summaries.iter().map(|(_, _, _, _, _, d, _)| d).sum();
    let total_items: usize = portfolio_summaries.iter().map(|(_, _, _, _, _, _, c)| c).sum();

    // ── Transaction category breakdown ──
    let txn_categories = features::sum_transactions_by_category(
        state.db(), &user.0, filter_from, filter_to, None, None, None,
    ).await.unwrap_or_default();
    let txn_cat_labels: Vec<String> = txn_categories.iter().map(|(cat, _, _)| cat.clone()).collect();
    let txn_cat_income: Vec<i64> = txn_categories.iter().map(|(_, inc, _)| *inc).collect();
    let txn_cat_expenses: Vec<i64> = txn_categories.iter().map(|(_, _, exp)| *exp).collect();

    // ── Change from first to last ──
    let first_net = net_worth_series.first().copied().unwrap_or(0);
    let last_net = net_worth_series.last().copied().unwrap_or(0);
    let net_change = last_net - first_net;
    let pct_change = if first_net != 0 { ((net_change as f64) / (first_net as f64).abs() * 100.0 * 10.0).round() / 10.0 } else { 0.0 };

    // ── Build filter query string for form ──

    Ok(active(
        "Stats",
        maud::html! {
            div class="page-header" {
                h2 { "Charts & Statistics" }
            }

            // ── Filter bar ──
            div class="filter-bar" {
                form method="get" action="/stats" class="filter-form" {
                    label { "Portfolio"
                        select name="portfolio" {
                            option value="" { "All Portfolios" }
                            @for (pid, pname) in &portfolios {
                                @let selected = filter_portfolio == Some(*pid);
                                option value=(pid.to_string()) selected[selected] { (pname) }
                            }
                        }
                    }
                    label { "From"
                        input type="date" name="from"
                               value=(filter.from.as_deref().unwrap_or("")) {}
                    }
                    label { "To"
                        input type="date" name="to"
                               value=(filter.to.as_deref().unwrap_or("")) {}
                    }
                    button type="submit" class="btn btn-primary btn-sm" { "Apply" }
                    a href="/stats" class="btn btn-sm" { "Reset" }
                }
            }

            // ── Summary cards ──
            div class="summary-cards" {
                div class="summary-card" {
                    span class="summary-label" { "Net Worth" }
                    span class=(format!("summary-value {}", if total_net_worth >= 0 { "positive" } else { "negative" })) { (format_cents(total_net_worth)) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Assets" }
                    span class="summary-value" { (format_cents(total_assets)) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Investments" }
                    span class="summary-value" { (format_cents(total_investments)) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Debts" }
                    span class="summary-value negative" { (format_cents(-total_debts)) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Portfolios" }
                    span class="summary-value" { (portfolios.len()) }
                }
                div class="summary-card" {
                    span class="summary-label" { "Items" }
                    span class="summary-value" { (total_items) }
                }
            }

            // ── Period change card ──
            @if !net_worth_series.is_empty() {
                div class="summary-cards" {
                    div class="summary-card" {
                        span class="summary-label" { "Period Change" }
                        span class=(format!("summary-value {}", if net_change >= 0 { "positive" } else { "negative" })) {
                            (format_cents(net_change))
                        }
                    }
                    div class="summary-card" {
                        span class="summary-label" { "Period %" }
                        span class=(format!("summary-value {}", if pct_change >= 0.0 { "positive" } else { "negative" })) {
                            (format!("{:.1}%", pct_change))
                        }
                    }
                }
            }

            // ── Charts ──
            div class="charts-grid" {
                div class="chart-card chart-wide" {
                    h3 { "Net Worth Over Time" }
                    div class="chart-container chart-container-lg" {
                        canvas id="netWorthChart" {}
                    }
                }
            }

            div class="charts-grid" {
                div class="chart-card" {
                    h3 { "Assets vs Debts Over Time" }
                    div class="chart-container" {
                        canvas id="stackedChart" {}
                    }
                }
                @if !type_totals.is_empty() {
                    div class="chart-card" {
                        h3 { "Category Breakdown" }
                        div class="chart-container" {
                            canvas id="categoryChart" {}
                        }
                    }
                }
            }

            @if !mom_change.is_empty() {
                div class="charts-grid" {
                    div class="chart-card chart-wide" {
                        h3 { "Month-over-Month Change" }
                        div class="chart-container chart-container-md" {
                            canvas id="momChart" {}
                        }
                    }
                }
            }

            @if !item_series.is_empty() {
                div class="charts-grid" {
                    div class="chart-card chart-wide" {
                        h3 { "Individual Items" }
                        div class="chart-container chart-container-lg" {
                            canvas id="itemsChart" {}
                        }
                    }
                }
            }

            // ── Portfolio breakdown ──
            @if !portfolio_summaries.is_empty() {
                div class="portfolio-summary-list" {
                    h3 { "Portfolio Breakdown" }
                    div class="stats-table-wrap" {
                        table class="stats-table" {
                            thead {
                                tr {
                                    th { "Portfolio" }
                                    th { "Net Worth" }
                                    th { "Assets" }
                                    th { "Investments" }
                                    th { "Debts" }
                                    th { "Items" }
                                }
                            }
                            tbody {
                                @for (pid, pname, nw, a, inv, d, cnt) in &portfolio_summaries {
                                    tr {
                                        td { a href=(format!("/portfolio/{}", pid)) { (pname) } }
                                        td class=(if *nw >= 0 { "positive" } else { "negative" }) { (format_cents(*nw)) }
                                        td { (format_cents(*a)) }
                                        td { (format_cents(*inv)) }
                                        td class="negative" { (format_cents(-*d)) }
                                        td { (cnt) }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Transaction category breakdown ──
            @if !txn_categories.is_empty() {
                div class="charts-grid" {
                    div class="chart-card" {
                        h3 { "Spending by Category" }
                        div class="chart-container" {
                            canvas id="txnCatChart" {}
                        }
                    }
                    div class="chart-card" {
                        h3 { "Income vs Expenses by Category" }
                        div class="chart-container" {
                            canvas id="txnBarChart" {}
                        }
                    }
                }
            }

            // ── Chart.js ──
            script src="/static/chart.min.js" defer {}
            script {
                (maud::PreEscaped(format!(
                    "const chartLabels={labels};const netWorthData={nw};const assetsData={assets};const investmentsData={inv};const debtsData={debts};const momLabels={ml};const momData={md};const typeLabels={tl};const typeValues={tv};const itemSeries={is};const txnCatLabels={tcl};const txnCatIncome={tci};const txnCatExpenses={tce};",
                    labels=serde_json::to_string(&chart_labels).unwrap(),
                    nw=serde_json::to_string(&net_worth_series).unwrap(),
                    assets=serde_json::to_string(&assets_series).unwrap(),
                    inv=serde_json::to_string(&investments_series).unwrap(),
                    debts=serde_json::to_string(&debts_series).unwrap(),
                    ml=serde_json::to_string(&mom_labels).unwrap(),
                    md=serde_json::to_string(&mom_change).unwrap(),
                    tl=serde_json::to_string(&type_labels).unwrap(),
                    tv=serde_json::to_string(&type_values).unwrap(),
                    is=serde_json::to_string(&item_series.iter().map(|(n,t,pts)|(n.clone(),t.clone(),pts.clone())).collect::<Vec<_>>()).unwrap(),
                    tcl=serde_json::to_string(&txn_cat_labels).unwrap(),
                    tci=serde_json::to_string(&txn_cat_income).unwrap(),
                    tce=serde_json::to_string(&txn_cat_expenses).unwrap(),
                )))
                (maud::PreEscaped(r#"
const dollarFmt={callbacks:{label:function(ctx){return '$'+(ctx.parsed.y/100).toFixed(2);}}};
const dollarAxis={ticks:{callback:function(v){return(v<0?'-':'')+'$'+Math.abs(v/100).toFixed(0);}},grid:{color:'#334155'}};

new Chart(document.getElementById('netWorthChart').getContext('2d'),{
  type:'line',data:{labels:chartLabels,datasets:[{label:'Net Worth',data:netWorthData,borderColor:'#3b82f6',backgroundColor:'rgba(59,130,246,0.08)',fill:true,tension:0.3,pointRadius:2,pointBackgroundColor:'#3b82f6'}]},
  options:{responsive:true,maintainAspectRatio:false,plugins:{tooltip:dollarFmt},scales:{y:dollarAxis,x:{grid:{color:'#334155'}}}}
});

new Chart(document.getElementById('stackedChart').getContext('2d'),{
  type:'line',data:{labels:chartLabels,datasets:[
    {label:'Assets',data:assetsData,borderColor:'#10b981',backgroundColor:'rgba(16,185,129,0.15)',fill:true,tension:0.3,pointRadius:1},
    {label:'Investments',data:investmentsData,borderColor:'#3b82f6',backgroundColor:'rgba(59,130,246,0.15)',fill:true,tension:0.3,pointRadius:1},
    {label:'Debts',data:debtsData,borderColor:'#f87171',backgroundColor:'rgba(248,113,113,0.15)',fill:true,tension:0.3,pointRadius:1}
  ]},
  options:{responsive:true,maintainAspectRatio:false,plugins:{tooltip:{callbacks:{label:function(ctx){return ctx.dataset.label+': $'+(ctx.parsed.y/100).toFixed(2);}}}},scales:{y:dollarAxis,x:{grid:{color:'#334155'}}}}
});

if(typeLabels.length>0){
  const catColors=['#10b981','#3b82f6','#f87171','#f59e0b','#8b5cf6'];
  const catBorder=['#059669','#2563eb','#dc2626','#d97706','#7c3aed'];
  new Chart(document.getElementById('categoryChart').getContext('2d'),{
    type:'doughnut',data:{labels:typeLabels.map(t=>t.charAt(0).toUpperCase()+t.slice(1)+'s'),datasets:[{data:typeValues.map(Math.abs),backgroundColor:catColors.slice(0,typeLabels.length),borderColor:catBorder.slice(0,typeLabels.length),borderWidth:2}]},
    options:{responsive:true,maintainAspectRatio:false,plugins:{legend:{position:'bottom',labels:{color:'#e2e8f0',padding:12}},tooltip:{callbacks:{label:function(ctx){return ctx.label+': $'+(ctx.parsed/100).toFixed(2);}}}}}
  });
}

if(momLabels.length>0){
  const momColors=momData.map(v=>v>=0?'rgba(16,185,129,0.7)':'rgba(248,113,113,0.7)');
  new Chart(document.getElementById('momChart').getContext('2d'),{
    type:'bar',data:{labels:momLabels,datasets:[{label:'MoM Change',data:momData,backgroundColor:momColors,borderWidth:0,borderRadius:4}]},
    options:{responsive:true,maintainAspectRatio:false,plugins:{tooltip:dollarFmt},scales:{y:{ticks:{callback:function(v){return '$'+(v/100).toFixed(0);}},grid:{color:'#334155'}},x:{grid:{color:'#334155'}}}}
  });
}

if(itemSeries.length>0){
  const ic=['#3b82f6','#10b981','#f87171','#f59e0b','#8b5cf6','#ec4899','#06b6d4','#84cc16','#f97316','#6366f1','#14b8a6','#e11d48'];
  const ds=itemSeries.map((item,i)=>({label:item[0],data:item[2].map(p=>p[1]),borderColor:ic[i%ic.length],backgroundColor:'transparent',tension:0.2,pointRadius:1,borderWidth:1.5}));
  new Chart(document.getElementById('itemsChart').getContext('2d'),{
    type:'line',data:{labels:itemSeries[0][2].map(p=>p[0]),datasets:ds},
    options:{responsive:true,maintainAspectRatio:false,plugins:{legend:{position:'bottom',labels:{color:'#e2e8f0',padding:8,font:{size:10}}},tooltip:{callbacks:{label:function(ctx){return ctx.dataset.label+': $'+(ctx.parsed.y/100).toFixed(2);}}}},scales:{y:{ticks:{callback:function(v){return '$'+(v/100).toFixed(0);}},grid:{color:'#334155'}},x:{grid:{color:'#334155'},ticks:{maxRotation:45,font:{size:10}}}}}
  });
}

// Transaction category charts
if(txnCatLabels.length>0){
  const catColors=['#3b82f6','#10b981','#f87171','#f59e0b','#8b5cf6','#ec4899','#06b6d4','#84cc16','#f97316','#6366f1','#14b8a6','#e11d48'];
  // Doughnut: total spending by category
  new Chart(document.getElementById('txnCatChart').getContext('2d'),{
    type:'doughnut',data:{labels:txnCatLabels,datasets:[{data:txnCatExpenses.map(Math.abs),backgroundColor:catColors.slice(0,txnCatLabels.length),borderColor:'#1e293b',borderWidth:2}]},
    options:{responsive:true,maintainAspectRatio:false,plugins:{legend:{position:'bottom',labels:{color:'#e2e8f0',padding:10}},tooltip:{callbacks:{label:function(ctx){return ctx.label+': $'+(ctx.parsed/100).toFixed(2);}}}}}
  });
  // Grouped bar: income vs expenses by category
  new Chart(document.getElementById('txnBarChart').getContext('2d'),{
    type:'bar',data:{labels:txnCatLabels,datasets:[
      {label:'Income',data:txnCatIncome,backgroundColor:'rgba(16,185,129,0.7)',borderRadius:4},
      {label:'Expenses',data:txnCatExpenses.map(v=>-v),backgroundColor:'rgba(248,113,113,0.7)',borderRadius:4}
    ]},
    options:{responsive:true,maintainAspectRatio:false,plugins:{legend:{position:'bottom',labels:{color:'#e2e8f0',padding:10}},tooltip:{callbacks:{label:function(ctx){return ctx.dataset.label+': $'+(Math.abs(ctx.parsed.y)/100).toFixed(2);}}}},scales:{y:{ticks:{callback:function(v){return '$'+(Math.abs(v)/100).toFixed(0);}},grid:{color:'#334155'}},x:{grid:{color:'#334155'},ticks:{font:{size:10}}}}}
  });
}
"#))
            }
        },
        Some(&user),
        false,
        "stats",
    ))
}

pub async fn not_found(State(_state): State<AppState>) -> impl IntoResponse {
    (
        axum::http::StatusCode::NOT_FOUND,
        layout(
            "Not Found",
            maud::html! {
                div class="empty-state" {
                    h1 { "404" }
                    p { "The page you're looking for doesn't exist." }
                    a href="/" class="btn" { "Go home" }
                }
            },
            None,
            false,
        ),
    )
}

pub async fn dashboard(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let hour = chrono::Local::now()
        .format("%H")
        .to_string()
        .parse::<u32>()
        .unwrap_or(12);
    let greeting = match hour {
        0..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };

    // Pull quick stats for the dashboard
    let pool = state.db();
    let portfolios = portfolio::list_portfolios(pool, &user.0).await?;
    let total_portfolios = portfolios.len();

    // Batch fetch all items and logs for user (avoids N+1 per-portfolio queries)
    let all_items = portfolio::list_all_wealth_items_for_user(pool, &user.0).await?;
    let all_logs = portfolio::list_all_balance_logs_for_user(pool, &user.0).await?;
    let mut latest_per_item: std::collections::HashMap<uuid::Uuid, (NaiveDate, i64)> = std::collections::HashMap::new();
    for l in &all_logs {
        let entry = latest_per_item.entry(l.item_id).or_insert((l.log_date, l.balance_value));
        if l.log_date > entry.0 { *entry = (l.log_date, l.balance_value); }
    }
    let mut total_net: i64 = 0;
    for item in &all_items {
        if let Some((_, val)) = latest_per_item.get(&item.item_id) {
            total_net += if item.item_type == "debt" { -val } else { *val };
        }
    }
    let total_items = all_items.len();

    let total_txns = crate::models::features::count_transactions(pool, &user.0, None, None, None, None, None).await?;
    let (month_income, month_expenses) = crate::models::features::sum_transactions(pool, &user.0, None, None, None, None, None).await?;

    let goals = crate::models::features::list_savings_goals(pool, &user.0).await?;
    let total_goals = goals.len();

    let net_cls = if total_net >= 0 { "positive" } else { "negative" };
    let month_net = month_income - month_expenses;
    let month_cls = if month_net >= 0 { "positive" } else { "negative" };

    Ok(active(
        "Dashboard",
        maud::html! {
            h2 class="greeting" { (greeting) ", " (user.0) }
            div class="dashboard-cards" {
                a href="/portfolios" class="card card-link" {
                    div class="card-icon" { "📊" }
                    h3 { "Portfolios" }
                    p { (total_portfolios) " portfolio" @if total_portfolios != 1 { "s" } ", " (total_items) " items" }
                    span class=(format!("card-stat {}", net_cls)) { (format_cents(total_net)) }
                }
                a href="/stats" class="card card-link" {
                    div class="card-icon" { "📈" }
                    h3 { "Statistics" }
                    p { "Charts, trends, and insights" }
                }
                a href="/transactions" class="card card-link" {
                    div class="card-icon" { "💳" }
                    h3 { "Transactions" }
                    p { (total_txns) " transactions" }
                    span class=(format!("card-stat {}", month_cls)) { (format_cents(month_net)) " net" }
                }
                a href="/budgets" class="card card-link" {
                    div class="card-icon" { "📋" }
                    h3 { "Budgets" }
                    p { "Monthly budget tracking" }
                }
                a href="/goals" class="card card-link" {
                    div class="card-icon" { "🎯" }
                    h3 { "Goals" }
                    p { (total_goals) " savings goal" @if total_goals != 1 { "s" } }
                }
                a href="/holidays" class="card card-link" {
                    div class="card-icon" { "🏖️" }
                    h3 { "Holidays" }
                    p { "Track holiday spending" }
                }
                a href="/reconciliation" class="card card-link" {
                    div class="card-icon" { "⚖️" }
                    h3 { "Reconciliation" }
                    p { "Balance changes vs flows" }
                }
            }
        },
        Some(&user),
        false,
        "dashboard",
    ))
}

pub async fn time(State(_state): State<AppState>) -> impl IntoResponse {
    maud::html! { p { "Time: " (chrono::Local::now().format("%H:%M:%S")) } }
}

// ── CSV Import ──

pub async fn portfolio_import(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    Ok(active(
        &format!("Import CSV - {}", name),
        maud::html! {
            div class="page-header" {
                a href=(format!("/portfolio/{}", portfolio_id)) class="back-link" { "← Back" }
                h2 { "Import CSV into " (name) }
            }
            div class="content-card" {
                h3 { "How it works" }
                ol {
                    li { "Upload a CSV file with dates in the first column and wealth item names as other column headers." }
                    li { "Choose the default item type for new items (asset, debt, investment)." }
                    li { "Values will be upserted — existing entries for the same date/item are updated." }
                }
                h4 { "Example CSV" }
                pre style="background:var(--bg-secondary);padding:1em;border-radius:8px;overflow-x:auto;font-size:0.9em;" {
                    "Date,Savings,Checking,Mortgage\n2025-01-01,10000,5000,150000\n2025-02-01,10200,4800,148000"
                }
                form method="post" action=(format!("/portfolio/{}/import", portfolio_id))
                      enctype="multipart/form-data"
                      class="form-grid" {
                    label { "CSV File"
                        input type="file" name="csv_file" accept=".csv,.txt" required {}
                    }
                    label { "Default Item Type"
                        select name="default_type" {
                            option value="asset" selected { "Asset" }
                            option value="investment" { "Investment" }
                            option value="debt" { "Debt" }
                        }
                    }
                    div class="form-actions" {
                        button type="submit" class="btn btn-primary" { "Import CSV" }
                        a href=(format!("/portfolio/{}", portfolio_id)) class="btn" { "Cancel" }
                    }
                }
            }
        },
        Some(&user),
        false,
        "portfolios",
    ))
}

pub async fn portfolio_import_post(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    mut multipart: axum::extract::Multipart,
) -> Result<axum::response::Redirect, AppError> {
    // Verify portfolio ownership
    portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;

    let mut csv_data = String::new();
    let mut default_type = "asset".to_string();
    let mut column_types: std::collections::HashMap<usize, String> = std::collections::HashMap::new();

    while let Some(field) = multipart.next_field().await.map_err(|e| {
        AppError::BadRequest(format!("Failed to read multipart field: {}", e))
    })? {
        let field_name = field.name().unwrap_or("").to_string();
        match field_name.as_str() {
            "csv_file" => {
                let bytes = field.bytes().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read file: {}", e))
                })?;
                csv_data = String::from_utf8(bytes.to_vec()).map_err(|e| {
                    AppError::BadRequest(format!("File is not valid UTF-8: {}", e))
                })?;
            }
            "default_type" => {
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read default_type: {}", e))
                })?;
                // Validate
                match text.as_str() {
                    "asset" | "investment" | "debt" => default_type = text,
                    _ => return Err(AppError::BadRequest(format!("Invalid item type: {}", text))),
                }
            }
            name if name.starts_with("column_") => {
                // column_1=debt, column_2=investment, etc.
                let text = field.text().await.map_err(|e| {
                    AppError::BadRequest(format!("Failed to read {}: {}", name, e))
                })?;
                if let Some(idx_str) = name.strip_prefix("column_") {
                    if let Ok(idx) = idx_str.parse::<usize>() {
                        match text.as_str() {
                            "asset" | "investment" | "debt" => {
                                column_types.insert(idx, text);
                            }
                            _ => {} // ignore invalid types
                        }
                    }
                }
            }
            _ => {} // ignore unknown fields
        }
    }

    if csv_data.is_empty() {
        return Err(AppError::BadRequest("No CSV file provided".into()));
    }

    let result = portfolio::import_csv(
        state.db(),
        portfolio_id,
        &csv_data,
        &default_type,
        &column_types,
    )
    .await?;

    let flash_msg = format!(
        "Imported {} rows ({} skipped, {} items created)",
        result.rows_imported, result.rows_skipped, result.items_created
    );

    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}?flash={}&flash_type=success",
        portfolio_id,
        urlencoding(&flash_msg)
    )))
}

/// Minimal percent-encoding for redirect URLs
fn urlencoding(s: &str) -> String {
    s.replace(' ', "+")
        .replace('%', "%25")
        .replace('#', "%23")
        .replace('&', "%26")
        .replace('?', "%3F")
}

// ── CSV Export ──

pub async fn portfolio_csv(
    Path(portfolio_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, &user.0).await?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;

    // Pivot: date -> item_id -> value
    let mut dates: BTreeMap<NaiveDate, std::collections::HashMap<uuid::Uuid, i64>> = BTreeMap::new();
    for log in &logs {
        dates
            .entry(log.log_date)
            .or_default()
            .insert(log.item_id, log.balance_value);
    }

    let mut wtr = csv::Writer::from_writer(Vec::new());
    // Header: Date,Item1,Item2,...
    let mut header = vec!["Date".to_string()];
    for item in &items {
        header.push(item.name.clone());
    }
    wtr.write_record(&header).map_err(|e| AppError::Internal(e.into()))?;

    for (date, values) in &dates {
        let mut row = vec![date.to_string()];
        for item in &items {
            let val = values.get(&item.item_id);
            match val {
                Some(cents) => row.push(utils::format_dollars(*cents)),
                None => row.push(String::new()),
            }
        }
        wtr.write_record(&row).map_err(|e| AppError::Internal(e.into()))?;
    }

    let data = wtr.into_inner().map_err(|e| AppError::Internal(e.into()))?;
    let filename = format!("attachment; filename=\"{}.csv\"", name);

    Ok((
        [
            ("content-type", "text/csv"),
            ("content-disposition", &filename),
        ],
        data,
    ).into_response())
}
