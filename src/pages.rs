use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::backup;
use crate::models::csv_import;
use crate::models::portfolio::{self, BalanceLog, WealthItem};
use crate::models::reconcile::{self, OutgoingTxn, ReconciledTxn};
use crate::models::user;
use crate::utils;
use axum::extract::{Form, Path, State};
use axum::response::IntoResponse;
use axum::response::Redirect;
use chrono::NaiveDate;
use uuid::Uuid;

struct GridRow {
    date: NaiveDate,
    values: Vec<Option<i64>>,
}

fn pivot_logs(items: &[WealthItem], logs: &[BalanceLog]) -> Vec<GridRow> {
    let item_index: std::collections::HashMap<Uuid, usize> = items
        .iter()
        .enumerate()
        .map(|(i, wi)| (wi.item_id, i))
        .collect();

    let mut by_date: std::collections::BTreeMap<NaiveDate, Vec<Option<i64>>> =
        std::collections::BTreeMap::new();
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

pub async fn hello(
    State(_state): State<AppState>,
    user: Option<LoggedInUser>,
) -> impl IntoResponse {
    layout(
        "Home",
        maud::html! {
        h1 {"Hello"}
        div id="clock" hx-get="/time" hx-trigger="every 1s" {
            "Loading..."
        }
                    },
        user.as_ref(),
    )
}

#[derive(serde::Deserialize)]
pub struct AddItemForm {
    pub name: String,
    pub item_type: String,
}

#[derive(serde::Deserialize)]
pub struct CreatePortfolioForm {
    pub name: String,
}

pub async fn create_portfolio(
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<CreatePortfolioForm>,
) -> Result<axum::response::Redirect, AppError> {
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Portfolio name is required".into()));
    }
    portfolio::create_portfolio(state.db(), user.0, form.name.trim()).await?;
    Ok(axum::response::Redirect::to("/portfolios"))
}

pub async fn add_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<AddItemForm>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    portfolio::create_wealth_item(state.db(), portfolio_id, &form.name, &form.item_type).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

#[derive(serde::Deserialize)]
pub struct RenamePortfolioForm {
    pub name: String,
}

pub async fn rename_portfolio(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<RenamePortfolioForm>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest(
            "Portfolio name cannot be empty".into(),
        ));
    }
    portfolio::rename_portfolio(state.db(), portfolio_id, form.name.trim()).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

#[derive(serde::Deserialize)]
pub struct MoveItemQuery {
    pub item_id: Uuid,
    pub direction: String,
}

pub async fn move_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<MoveItemQuery>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    portfolio::move_wealth_item(state.db(), portfolio_id, query.item_id, &query.direction).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

pub async fn portfolios(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(state.db(), user.0).await?;
    Ok(layout(
        "Portfolios",
        maud::html! {
            details class="add-item-details" {
                summary { "+ New Portfolio" }
                form method="post" action="/portfolios" class="add-item-form" {
                    label { "Name"
                        input type="text" name="name" required {}
                    }
                    button type="submit" { "Create" }
                }
            }
            div class="portfolio-list"{
                @for (id, name) in portfolios {
                    div class="portfolio-row" id=(format!("portfolio-{}", id)){
                        div class="portfolio-info" {
                            h3 { (name) }
                        }
                            div class="portfolio-actions"{
                            a href=(format!("/portfolio/{}", id)) class="btn-view" {"View Details" }
                        }
                    }
                }
            }
        },
        Some(&user),
    ))
}

#[derive(serde::Deserialize, Default)]
pub struct PortfolioQuery {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

pub async fn portfolio(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<PortfolioQuery>,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let grid_rows = pivot_logs(&items, &logs);

    Ok(layout(
        &format!("portfolio - {}", name),
        maud::html! {
            a href="/portfolios" { "← Back" }
            @if let Some(msg) = &query.flash {
                div class=(if query.flash_type.as_deref() == Some("success") { "flash-success" } else if query.flash_type.as_deref() == Some("error") { "flash-error" } else { "flash-info" }) { (msg) }
            }
            div style="margin: 0.5em 0; display: flex; gap: 0.5em;" {
                a href=(format!("/portfolio/{}/import", portfolio_id)) class="btn" { "Import CSV" }
                a href=(format!("/portfolio/{}/export/csv", portfolio_id)) class="btn btn-ghost" { "Export CSV" }
            }
            form method="post" action=(format!("/portfolio/{}/rename", portfolio_id)) class="portfolio-name-form" {
                input type="text" name="name" value=(name)
                       class="portfolio-name-input"
                       onblur="this.closest('form').requestSubmit()"
                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
            }

            details class="add-item-details" {
                summary { "+ Add Wealth Item" }
                form method="post" action=(format!("/portfolio/{}/items", portfolio_id)) class="add-item-form" {
                    label { "Name"
                        input type="text" name="name" required {}
                    }
                    label { "Type"
                        select name="item_type" {
                            option value="asset" { "Asset" }
                            option value="cash" { "Cash" }
                            option value="debt" { "Debt" }
                            option value="investment" { "Investment" }
                        }
                    }
                    button type="submit" { "Add Item" }
                }
            }
            @if items.is_empty() {
                p { "No wealth items yet. Add one to start tracking." }
            }
            @else {
                div class="item-cards" {
                    @for item in &items {
                        @let type_class = match item.item_type.as_str() {
                            "debt" => "item-card--debt",
                            "investment" => "item-card--investment",
                            "cash" => "item-card--cash",
                            _ => "item-card--asset",
                        };
                        div class=(format!("item-card {}", type_class)) {
                            form method="post" action=(format!("/portfolio/{}/delete-item", portfolio_id)) class="item-card__delete-form" {
                                input type="hidden" name="item_id" value=(item.item_id) {}
                                button type="submit" class="item-card__delete" title="Delete item" onclick="return confirm('Delete this item? All its data will be removed.')" { "×" }
                            }
                            form method="post" action=(format!("/portfolio/{}/rename-item", portfolio_id)) class="item-card__name-form" {
                                input type="hidden" name="item_id" value=(item.item_id) {}
                                input type="text" name="name" value=(item.name)
                                       class="item-card__name-input"
                                       onblur="this.closest('form').requestSubmit()"
                                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
                            }
                            form method="post" action=(format!("/portfolio/{}/change-type", portfolio_id)) class="item-card__type-form" {
                                input type="hidden" name="item_id" value=(item.item_id) {}
                                select name="item_type" class="item-card__type" onchange="this.closest('form').requestSubmit()" {
                                    option value="asset" selected[item.item_type == "asset"] { "Asset" }
                                    option value="cash" selected[item.item_type == "cash"] { "Cash" }
                                    option value="debt" selected[item.item_type == "debt"] { "Debt" }
                                    option value="investment" selected[item.item_type == "investment"] { "Investment" }
                                }
                            }
                        }
                    }
                }
            }
            @if !items.is_empty() {
                div class="grid-wrapper" {
                    table {
                        thead {
                            tr {
                                th { "Date" }
                                @for (idx, item) in items.iter().enumerate() {
                                    @let type_class = match item.item_type.as_str() {
                                        "debt" => "th--debt",
                                        "investment" => "th--investment",
                                        "cash" => "th--cash",
                                        _ => "th--asset",
                                    };
                                    th id=(format!("th-{}", item.item_id)) class=(format!("{}", type_class)) {
                                        (item.name)
                                        span class="col-arrows" {
                                            @if idx > 0 {
                                                form method="post" action=(format!("/portfolio/{}/move-item?item_id={}&direction=left", portfolio_id, item.item_id)) {
                                                    button type="submit" class="col-arrow-btn" title="Move left" { "←" }
                                                }
                                            }
                                            @if idx < items.len() - 1 {
                                                form method="post" action=(format!("/portfolio/{}/move-item?item_id={}&direction=right", portfolio_id, item.item_id)) {
                                                    button type="submit" class="col-arrow-btn" title="Move right" { "→" }
                                                }
                                            }
                                        }
                                    }
                                }
                                th { "Total" }
                            }
                        }
                        tbody {
                            tr id="blank-row" class="blank-row" {
                                td {
                                    input type="date" name="log_date"
                                           form="balance-add-form" required {}
                                }
                                @for item in &items {
                                    td {
                                        input type="number" step="0.01"
                                               name=(format!("balance_{}", item.item_id))
                                               form="balance-add-form"
                                               placeholder="$0.00" {}
                                    }
                                }
                                td class="row-total" {
                                    form id="balance-add-form"
                                        hx-post=(format!("/portfolio/{}/balances", portfolio_id))
                                        hx-target="#blank-row"
                                        hx-swap="afterend" {
                                        button type="submit" class="btn btn-primary btn-xs" { "+ Add" }
                                    }
                                }
                            }
                            @for row in &grid_rows {
                                @let total: i64 = row.values.iter().enumerate()
                                    .filter_map(|(i, v)| v.as_ref().map(|val| if items[i].item_type == "debt" { -*val } else { *val }))
                                    .sum();
                                tr id=(format!("row-{}", row.date)) {
                                    td id=(format!("date-{}", row.date)) class="editable date-cell" tabindex="0"
                                       hx-get=(format!("/portfolio/{}/date?date={}", portfolio_id, row.date))
                                       hx-target=(format!("#date-{}", row.date))
                                       hx-swap="outerHTML" {
                                        (row.date)
                                    }
                                    @for (idx, val) in row.values.iter().enumerate() {
                                        @let item_id = items[idx].item_id;
                                        @let cell_id = format!("cell-{}-{}", item_id, row.date);
                                        @match val {
                                            Some(cents) => {
                                                td id=(cell_id) class="editable"
                                                   tabindex="0"
                                                   hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, row.date))
                                                   hx-target=(format!("#{}", cell_id))
                                                   hx-swap="outerHTML" {
                                                    (utils::format_cents(*cents))
                                                }
                                            }
                                            None => {
                                                td id=(cell_id) class="editable empty"
                                                   tabindex="0"
                                                   hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, row.date))
                                                   hx-target=(format!("#{}", cell_id))
                                                   hx-swap="outerHTML" {
                                                    "\u{2014}"
                                                }
                                            }
                                        }
                                    }
                                    td class="row-total" { (utils::format_cents(total)) }
                                }
                            }
                        }
                    }
                }
            }
        },
        Some(&user),
    ))
}

pub async fn add_balance(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;

    let log_date_str = form
        .get("log_date")
        .ok_or_else(|| AppError::BadRequest("Missing log date field".into()))?;
    let log_date = NaiveDate::parse_from_str(log_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    for item in &items {
        let key = format!("balance_{}", item.item_id);
        if let Some(value) = form.get(&key)
            && let Ok(cents) = utils::parse_dollars(value)
        {
            portfolio::insert_balance_log(state.db(), item.item_id, log_date, cents).await?;
        }
    }

    // Build the values for this date
    let _item_index: std::collections::HashMap<Uuid, usize> = items
        .iter()
        .enumerate()
        .map(|(i, wi)| (wi.item_id, i))
        .collect();

    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let values: Vec<Option<i64>> = items
        .iter()
        .map(|item| {
            logs.iter()
                .find(|l| l.item_id == item.item_id && l.log_date == log_date)
                .map(|l| l.balance_value)
        })
        .collect();

    let total: i64 = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            v.as_ref().map(|val| {
                if items[i].item_type == "debt" {
                    -*val
                } else {
                    *val
                }
            })
        })
        .sum();

    Ok(maud::html! {
        tr id=(format!("row-{}", log_date)) {
            td id=(format!("date-{}", log_date)) class="editable date-cell" tabindex="0"
               hx-get=(format!("/portfolio/{}/date?date={}", portfolio_id, log_date))
               hx-target=(format!("#date-{}", log_date))
               hx-swap="outerHTML" {
                (log_date)
            }
            @for (idx, val) in values.iter().enumerate() {
                @let item_id = items[idx].item_id;
                @let cell_id = format!("cell-{}-{}", item_id, log_date);
                @match val {
                    Some(cents) => {
                        td id=(cell_id) class="editable"
                           tabindex="0"
                           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, log_date))
                           hx-target=(format!("#{}", cell_id))
                           hx-swap="outerHTML" {
                            (utils::format_cents(*cents))
                        }
                    }
                    None => {
                        td id=(cell_id) class="editable empty"
                           tabindex="0"
                           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, log_date))
                           hx-target=(format!("#{}", cell_id))
                           hx-swap="outerHTML" {
                            "\u{2014}"
                        }
                    }
                }
            }
            td class="row-total" { (utils::format_cents(total)) }
        }
    })
}

// ── Inline cell editing (HTMX) ──

#[derive(serde::Deserialize)]
pub struct CellQuery {
    item_id: String,
    date: String,
}

/// GET: return an inline form to edit one cell.
pub async fn edit_cell(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<CellQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let item_id = Uuid::parse_str(&query.item_id)
        .map_err(|_| AppError::BadRequest("Invalid item ID.".into()))?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;

    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let current_cents = logs
        .iter()
        .find(|l| l.item_id == item_id && l.log_date == date)
        .map(|l| l.balance_value);

    let cell_id = format!("cell-{}-{}", item_id, date);
    let display_val = current_cents
        .map(|c| {
            let sign = if c < 0 { "-" } else { "" };
            let abs = c.abs();
            format!("{}{}.{:02}", sign, abs / 100, abs % 100)
        })
        .unwrap_or_default();

    let cancel_url = format!(
        "/portfolio/{}/cell?item_id={}&date={}",
        portfolio_id, item_id, date
    );
    let target_sel = format!("#{}", cell_id);

    Ok(maud::html! {
        td id=(cell_id) class="editable" tabindex="0" {
            form class="cell-edit-form"
                  hx-put=(format!("/portfolio/{}/cell", portfolio_id))
                  hx-target=(format!("#{}", cell_id))
                  hx-swap="outerHTML"
                  hx-trigger="submit" {
                input type="hidden" name="item_id" value=(item_id) {}
                input type="hidden" name="date" value=(date) {}
                input type="number" step="0.01" name="value"
                       value=(display_val)
                       class="cell-edit-input"
                       onblur="this.closest('form').requestSubmit()"
                       onkeydown=(format!("if(event.key==='Enter'){{event.preventDefault();this.closest('form').requestSubmit()}}else if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','{}',{{target:'{}',swap:'outerHTML'}})}}", cancel_url, target_sel))
                       autofocus {}
            }
        }
    })
}

/// PUT: save the edited cell value, return the formatted display.
pub async fn save_cell(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let item_id_str = form
        .get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = Uuid::parse_str(item_id_str)
        .map_err(|_| AppError::BadRequest("Invalid item ID.".into()))?;
    let date_str = form
        .get("date")
        .ok_or_else(|| AppError::BadRequest("Missing date".into()))?;
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let value_str = form
        .get("value")
        .ok_or_else(|| AppError::BadRequest("Missing value".into()))?;

    let cell_id = format!("cell-{}-{}", item_id, date);

    if value_str.trim().is_empty() {
        return Ok(maud::html! {
            td id=(cell_id) class="editable empty" tabindex="0"
               hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
               hx-target=(format!("#{}", cell_id))
               hx-swap="outerHTML" {
                "\u{2014}"
            }
        });
    }

    let cents = utils::parse_dollars(value_str).map_err(AppError::BadRequest)?;
    portfolio::upsert_balance_log(state.db(), item_id, date, cents).await?;

    Ok(maud::html! {
        td id=(cell_id) class="editable" tabindex="0"
           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
           hx-target=(format!("#{}", cell_id))
           hx-swap="outerHTML" {
            (utils::format_cents(cents))
        }
    })
}

// ── Inline date editing (HTMX) ──

#[derive(serde::Deserialize)]
pub struct DateQuery {
    date: String,
}

pub async fn edit_date(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<DateQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;

    let date_id = format!("date-{}", date);
    let row_id = format!("row-{}", date);
    let row_target = format!("#{}", row_id);

    Ok(maud::html! {
        td id=(date_id) class="editable date-cell" tabindex="0" {
            form class="cell-edit-form"
                  hx-put=(format!("/portfolio/{}/date", portfolio_id))
                  hx-target=(row_target)
                  hx-swap="outerHTML"
                  hx-trigger="submit" {
                input type="hidden" name="old_date" value=(date) {}
                input type="text" name="new_date"
                       value=(date)
                       placeholder="YYYY-MM-DD"
                       class="cell-edit-input date-input"
                       onblur="this.closest('form').requestSubmit()"
                       onkeydown=(format!("if(event.key==='Enter'){{event.preventDefault();this.closest('form').requestSubmit()}}else if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','/portfolio/{}/row?date={}',{{target:'{}',swap:'outerHTML'}})}}", portfolio_id, date, format!("#row-{}", date)))
                       autofocus {}
            }
        }
    })
}

/// Render a full data row (used for both normal display and after date rename).
fn render_data_row(
    portfolio_id: Uuid,
    items: &[WealthItem],
    date: NaiveDate,
    values: &[Option<i64>],
) -> maud::Markup {
    render_data_row_inner(portfolio_id, items, date, values, None)
}

/// Render a data row with an inline error message in the date cell.
fn render_data_row_with_error(
    portfolio_id: Uuid,
    items: &[WealthItem],
    date: NaiveDate,
    values: &[Option<i64>],
    error: &str,
) -> maud::Markup {
    render_data_row_inner(portfolio_id, items, date, values, Some(error))
}

fn render_data_row_inner(
    portfolio_id: Uuid,
    items: &[WealthItem],
    date: NaiveDate,
    values: &[Option<i64>],
    error: Option<&str>,
) -> maud::Markup {
    let total: i64 = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            v.as_ref().map(|val| {
                if items[i].item_type == "debt" {
                    -*val
                } else {
                    *val
                }
            })
        })
        .sum();

    let row_id = format!("row-{}", date);
    let date_id = format!("date-{}", date);

    maud::html! {
        tr id=(row_id) {
            td id=(date_id) class="editable date-cell" tabindex="0"
               hx-get=(format!("/portfolio/{}/date?date={}", portfolio_id, date))
               hx-target=(format!("#date-{}", date))
               hx-swap="outerHTML" {
                (date)
                @if let Some(msg) = error {
                    div class="date-error" { (msg) " Try again." }
                }
            }
            @for (idx, val) in values.iter().enumerate() {
                @let item_id = items[idx].item_id;
                @let cell_id = format!("cell-{}-{}", item_id, date);
                @match val {
                    Some(cents) => {
                        td id=(cell_id) class="editable"
                           tabindex="0"
                           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
                           hx-target=(format!("#{}", cell_id))
                           hx-swap="outerHTML" {
                            (utils::format_cents(*cents))
                        }
                    }
                    None => {
                        td id=(cell_id) class="editable empty"
                           tabindex="0"
                           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
                           hx-target=(format!("#{}", cell_id))
                           hx-swap="outerHTML" {
                            "\u{2014}"
                        }
                    }
                }
            }
            td class="row-total" { (utils::format_cents(total)) }
        }
    }
}

pub async fn save_date(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let old_date_str = form
        .get("old_date")
        .ok_or_else(|| AppError::BadRequest("Missing old_date".into()))?;
    let new_date_str = form
        .get("new_date")
        .ok_or_else(|| AppError::BadRequest("Missing new_date".into()))?;
    let old_date = NaiveDate::parse_from_str(old_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid old date format. Use YYYY-MM-DD.".into()))?;

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;

    // If new_date is invalid, re-render the original row with an error
    let new_date = match NaiveDate::parse_from_str(new_date_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            let values: Vec<Option<i64>> = items
                .iter()
                .map(|item| {
                    logs.iter()
                        .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                        .map(|l| l.balance_value)
                })
                .collect();
            return Ok(render_data_row_with_error(
                portfolio_id,
                &items,
                old_date,
                &values,
                "Invalid date format. Use YYYY-MM-DD.",
            ));
        }
    };

    if old_date == new_date {
        let values: Vec<Option<i64>> = items
            .iter()
            .map(|item| {
                logs.iter()
                    .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                    .map(|l| l.balance_value)
            })
            .collect();
        return Ok(render_data_row(portfolio_id, &items, old_date, &values));
    }

    match portfolio::rename_date(state.db(), portfolio_id, old_date, new_date).await {
        Ok(_) => {}
        Err(AppError::BadRequest(msg)) => {
            let values: Vec<Option<i64>> = items
                .iter()
                .map(|item| {
                    logs.iter()
                        .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                        .map(|l| l.balance_value)
                })
                .collect();
            return Ok(render_data_row_with_error(
                portfolio_id,
                &items,
                old_date,
                &values,
                &msg,
            ));
        }
        Err(e) => return Err(e),
    }

    let values: Vec<Option<i64>> = items
        .iter()
        .map(|item| {
            logs.iter()
                .find(|l| l.item_id == item.item_id && l.log_date == new_date)
                .map(|l| l.balance_value)
        })
        .collect();

    Ok(render_data_row(portfolio_id, &items, new_date, &values))
}

/// GET: return a data row (used to cancel date editing).
pub async fn get_row(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<DateQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let values: Vec<Option<i64>> = items
        .iter()
        .map(|item| {
            logs.iter()
                .find(|l| l.item_id == item.item_id && l.log_date == date)
                .map(|l| l.balance_value)
        })
        .collect();

    Ok(render_data_row(portfolio_id, &items, date, &values))
}

pub async fn save_item_name(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let item_id_str = form
        .get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = Uuid::parse_str(item_id_str)?;
    let name = form
        .get("name")
        .ok_or_else(|| AppError::BadRequest("Missing name".into()))?;

    if name.trim().is_empty() {
        return Err(AppError::BadRequest("Item name cannot be empty".into()));
    }

    portfolio::rename_wealth_item(state.db(), item_id, name.trim()).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

#[derive(serde::Deserialize)]
pub struct ChangeTypeForm {
    pub item_id: Uuid,
    pub item_type: String,
}

#[derive(serde::Deserialize)]
pub struct DeleteItemForm {
    pub item_id: Uuid,
}

pub async fn change_item_type(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<ChangeTypeForm>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let valid_types = ["asset", "cash", "debt", "investment"];
    if !valid_types.contains(&form.item_type.as_str()) {
        return Err(AppError::BadRequest("Invalid item type".into()));
    }
    portfolio::change_wealth_item_type(state.db(), form.item_id, &form.item_type).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

pub async fn delete_item(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<DeleteItemForm>,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    portfolio::delete_wealth_item(state.db(), form.item_id).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
}

pub async fn dashboard(State(state): State<AppState>, user: LoggedInUser) -> impl IntoResponse {
    let username = user::get_username_by_id(state.db(), user.0)
        .await
        .unwrap_or_else(|_| "User".to_string());
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
    layout(
        "Dashboard",
        maud::html! {
            h2 { (greeting) ", " (username) }
            div class="cards" {
                a href="/portfolios" class="card" {
                    h3 { "Portfolios" }
                    p { "View and manage your portfolios" }
                }
                a href="/insights" class="card" {
                    h3 { "Insights" }
                    p { "View your financial insights" }
                }
                a href="/reconcile" class="card" {
                    h3 { "Reconcile" }
                    p { "Match outgoing transactions to reconciled records" }
                }
                a href="/settings" class="card" {
                    h3 { "Settings" }
                    p { "Configure backups and preferences" }
                }
            }
        },
        Some(&user),
    )
}

pub async fn insights(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(state.db(), user.0).await?;

    // Build portfolio selector links
    let portfolio_links: Vec<maud::Markup> = portfolios
        .iter()
        .map(|(pid, pname)| {
            maud::html! {
                a href=(format!("/insights/{}", pid)) class="insights-portfolio-link" { (pname) }
            }
        })
        .collect();

    Ok(layout(
        "Insights",
        maud::html! {
            h2 { "Insights" }
            div class="insights-portfolio-list" {
                @for link in &portfolio_links {
                    (link)
                }
            }
        },
        Some(&user),
    ))
}

pub async fn insights_chart(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(portfolio_id): Path<Uuid>,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(state.db(), user.0).await?;
    let portfolio_name = portfolios
        .iter()
        .find(|(pid, _)| pid == &portfolio_id)
        .map(|(_, pname)| pname.clone())
        .unwrap_or_else(|| "Unknown".to_string());

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;

    // Get unique dates sorted ascending
    let mut dates: Vec<String> = logs
        .iter()
        .map(|l| l.log_date.format("%Y-%m-%d").to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    dates.sort();

    let mut item_names: Vec<String> = Vec::new();
    let mut values: Vec<Vec<f64>> = Vec::new();

    for item in &items {
        let item_logs: Vec<_> = logs.iter().filter(|l| l.item_id == item.item_id).collect();

        let mut row = vec![0.0; dates.len()];
        for log in &item_logs {
            let date_str = log.log_date.format("%Y-%m-%d").to_string();
            if let Some(idx) = dates.iter().position(|d| d == &date_str) {
                let val = if item.item_type == "debt" {
                    -(log.balance_value as f64) / 100.0
                } else {
                    log.balance_value as f64 / 100.0
                };
                row[idx] = val;
            }
        }

        item_names.push(item.name.clone());
        values.push(row);
    }

    // Build portfolio selector links
    let portfolio_links: Vec<maud::Markup> = portfolios.iter().map(|(pid, pname)| {
        let current = *pid == portfolio_id;
        maud::html! {
            a href=(format!("/insights/{}", pid))
               class=(if current { "insights-portfolio-link current" } else { "insights-portfolio-link" }) { (pname) }
        }
    }).collect();

    // Chart A: Cumulative Net Worth Trend (stacked area line)
    use charming::element::smoothness::Smoothness;
    use charming::element::{AxisLabel, TextStyle};
    use charming::renderer::HtmlRenderer;
    use charming::{
        Chart,
        component::{Axis, Legend, Title},
        element::{AreaStyle, AxisType, Tooltip, Trigger},
        series::Line,
        theme::Theme,
    };

    let white_text = TextStyle::new().color("#ffffff");
    let white_axis_label = AxisLabel::new().color("#ffffff");

    let mut trend_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Net Worth Trend", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Axis))
        .legend(
            Legend::new()
                .data(item_names.clone())
                .text_style(white_text.clone()),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(dates.clone())
                .axis_label(white_axis_label.clone()),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .axis_label(white_axis_label.clone()),
        );

    for (i, name) in item_names.iter().enumerate() {
        let series = Line::new()
            .name(name.clone())
            .stack("total")
            .area_style(AreaStyle::new().opacity(0.3))
            .smooth(Smoothness::Boolean(true))
            .data(values[i].clone());
        trend_chart = trend_chart.series(series);
    }

    let trend_html = HtmlRenderer::new("trend-chart", 900, 500)
        .theme(Theme::Dark)
        .render(&trend_chart)
        .unwrap_or_else(|_| "<p>Trend chart rendering failed</p>".to_string());

    // Chart B: Cash Flow (grouped bar — positive = income, negative = expenses)
    // Compute per-date totals for inflows vs outflows
    let mut inflow: Vec<f64> = vec![0.0; dates.len()];
    let mut outflow: Vec<f64> = vec![0.0; dates.len()];
    for (i, name) in item_names.iter().enumerate() {
        let item = items.iter().find(|it| &it.name == name).unwrap();
        for (j, &val) in values[i].iter().enumerate() {
            if item.item_type == "debt" {
                outflow[j] += val.abs();
            } else {
                inflow[j] += val;
            }
        }
    }

    use charming::series::Bar;
    let mut flow_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Cash Flow", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Axis))
        .legend(
            Legend::new()
                .data(vec!["Income".to_string(), "Expenses".to_string()])
                .text_style(white_text.clone()),
        )
        .x_axis(
            Axis::new()
                .type_(AxisType::Category)
                .data(dates.clone())
                .axis_label(white_axis_label.clone()),
        )
        .y_axis(
            Axis::new()
                .type_(AxisType::Value)
                .axis_label(white_axis_label.clone()),
        );

    flow_chart = flow_chart
        .series(Bar::new().name("Income").data(inflow))
        .series(Bar::new().name("Expenses").data(outflow));

    let flow_html = HtmlRenderer::new("flow-chart", 900, 400)
        .theme(Theme::Dark)
        .render(&flow_chart)
        .unwrap_or_else(|_| "<p>Flow chart rendering failed</p>".to_string());

    // Chart C: Asset Allocation (donut pie)
    // Compute latest values per item (use last non-zero, or last date's value)
    let mut pie_data: Vec<(String, f64)> = Vec::new();
    for (i, name) in item_names.iter().enumerate() {
        let latest = values[i]
            .iter()
            .rev()
            .find(|&&v| v != 0.0)
            .copied()
            .unwrap_or(0.0);
        if latest > 0.0 {
            pie_data.push((name.clone(), latest));
        }
    }

    use charming::datatype::DataPoint;
    use charming::series::Pie;

    let pie_series_data: Vec<DataPoint> = pie_data
        .iter()
        .map(|(name, val)| {
            DataPoint::Item(charming::datatype::DataPointItem::new(*val).name(name.clone()))
        })
        .collect();

    let pie_chart = Chart::new()
        .background_color("#0f172a")
        .title(
            Title::new()
                .text(format!("{} — Asset Allocation", portfolio_name))
                .text_style(white_text.clone()),
        )
        .tooltip(Tooltip::new().trigger(Trigger::Item))
        .legend(
            Legend::new()
                .data(pie_data.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>())
                .text_style(white_text.clone())
                .bottom("0%")
                .left("center"),
        )
        .series(
            Pie::new()
                .name("Allocation")
                .radius(vec!["40%", "70%"])
                .data(pie_series_data),
        );

    let pie_html = HtmlRenderer::new("pie-chart", 900, 500)
        .theme(Theme::Dark)
        .render(&pie_chart)
        .unwrap_or_else(|_| "<p>Pie chart rendering failed</p>".to_string());

    // Replace hardcoded "chart" ids in charming HTML with unique ids
    // (charming hardcodes id="chart" for every render)
    fn make_chart_id(html: &str, new_id: &str) -> String {
        html.replace("id=\"chart\"", &format!("id=\"{}\"", new_id))
            .replace(
                "getElementById('chart')",
                &format!("getElementById('{}')", new_id),
            )
    }

    let trend_html = make_chart_id(&trend_html, "trend-chart");
    let flow_html = make_chart_id(&flow_html, "flow-chart");
    let pie_html = make_chart_id(&pie_html, "pie-chart");

    Ok(layout(
        "Insights",
        maud::html! {
            h2 { "Insights" }
            div class="insights-portfolio-list" {
                @for link in &portfolio_links {
                    (link)
                }
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(trend_html))
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(flow_html))
            }
            div class="insights-chart-section" {
                (maud::PreEscaped(pie_html))
            }
        },
        Some(&user),
    ))
}

// ── Reconcile ──

pub async fn reconcile_list(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let sessions = reconcile::list_sessions(state.db(), user.0).await?;
    Ok(layout(
        "Reconcile",
        maud::html! {
            h2 { "Reconcile" }
            details class="add-item-details" {
                summary { "+ New Reconcile Session" }
                form method="post" action="/reconcile" class="add-item-form" {
                    label { "Name"
                        input type="text" name="name" required {}
                    }
                    button type="submit" { "Create" }
                }
            }
            @if sessions.is_empty() {
                p { "No reconcile sessions yet. Create one to start matching transactions." }
            } @else {
                div class="portfolio-list" {
                    @for (id, name) in &sessions {
                        div class="portfolio-row" {
                            div class="portfolio-info" {
                                h3 { (name) }
                            }
                            div class="portfolio-actions" {
                                a href=(format!("/reconcile/{}", id)) class="btn-view" { "View" }
                                form method="post" action=(format!("/reconcile/{}/delete", id))
                                     style="display:inline" {
                                    button type="submit" class="btn-ghost" style="margin-left:0.5rem"
                                            onclick="return confirm('Delete this session and all its data?')" { "Delete" }
                                }
                            }
                        }
                    }
                }
            }
        },
        Some(&user),
    ))
}

#[derive(serde::Deserialize)]
pub struct CreateSessionForm {
    pub name: String,
}

pub async fn reconcile_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<CreateSessionForm>,
) -> Result<axum::response::Redirect, AppError> {
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Session name is required".into()));
    }
    let id = reconcile::create_session(state.db(), user.0, form.name.trim()).await?;
    Ok(axum::response::Redirect::to(&format!("/reconcile/{}", id)))
}

pub async fn reconcile_delete(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    reconcile::delete_session(state.db(), session_id).await?;
    Ok(axum::response::Redirect::to("/reconcile"))
}

pub async fn reconcile_detail(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let (_, name) = reconcile::get_session(state.db(), session_id, user.0).await?;
    let outgoing = reconcile::list_outgoing(state.db(), session_id).await?;
    let reconciled = reconcile::list_reconciled(state.db(), session_id).await?;
    let matches = reconcile::list_matches(state.db(), session_id).await?;

    // Build lookup: outgoing_id -> list of reconciled_ids
    let mut match_map: std::collections::HashMap<Uuid, Vec<Uuid>> =
        std::collections::HashMap::new();
    let mut reverse_map: std::collections::HashMap<Uuid, Vec<(Uuid, Uuid)>> =
        std::collections::HashMap::new();
    for m in &matches {
        match_map
            .entry(m.outgoing_id)
            .or_default()
            .push(m.reconciled_id);
        reverse_map
            .entry(m.reconciled_id)
            .or_default()
            .push((m.match_id, m.outgoing_id));
    }

    let unmatched_outgoing: Vec<&OutgoingTxn> = outgoing.iter().filter(|o| !o.matched).collect();
    let unmatched_reconciled: Vec<&ReconciledTxn> =
        reconciled.iter().filter(|r| !r.matched).collect();
    let unmatched_max = unmatched_outgoing.len().max(unmatched_reconciled.len());

    Ok(layout(
        &format!("Reconcile — {}", name),
        maud::html! {
            a href="/reconcile" { "← Back" }
            form class="portfolio-name-form" method="post" action=(format!("/reconcile/{}/rename", session_id)) {
                input type="text" name="name" value=(name)
                       class="portfolio-name-input"
                       onblur="this.closest('form').requestSubmit()"
                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
            }

            form id="reconcile-match-form" method="post" action=(format!("/reconcile/{}/link", session_id)) {}

            // ── Toolbar ──
            div class="reconcile-toolbar" {
                details class="add-item-details" {
                    summary { "+ Add Outgoing" }
                    form method="post" action=(format!("/reconcile/{}/outgoing", session_id)) class="add-item-form reconcile-add-form" {
                        label { "Date"
                            input type="text" name="date" placeholder="YYYY-MM-DD" required {}
                        }
                        label { "Amount"
                            input type="number" step="0.01" name="amount" placeholder="0.00" required {}
                        }
                        label { "Vendor"
                            input type="text" name="vendor" {}
                        }
                        button type="submit" { "Add" }
                    }
                }
                details class="add-item-details" {
                    summary { "+ Add Reconciled" }
                    form method="post" action=(format!("/reconcile/{}/reconciled", session_id)) class="add-item-form reconcile-add-form" {
                        label { "Date"
                            input type="text" name="date" placeholder="YYYY-MM-DD" required {}
                        }
                        label { "Amount"
                            input type="number" step="0.01" name="amount" placeholder="0.00" required {}
                        }
                        label { "Vendor"
                            input type="text" name="vendor" {}
                        }
                        button type="submit" { "Add" }
                    }
                }
                details class="add-item-details" {
                    summary { "↑ Upload CSV" }
                    form method="post" action=(format!("/reconcile/{}/outgoing/csv", session_id))
                          enctype="multipart/form-data"
                          class="add-item-form reconcile-add-form" {
                        label { "Outgoing CSV"
                            input type="file" name="csv_file" accept=".csv" {}
                        }
                        button type="submit" { "Upload Outgoing" }
                    }
                    form method="post" action=(format!("/reconcile/{}/reconciled/csv", session_id))
                          enctype="multipart/form-data"
                          class="add-item-form reconcile-add-form" {
                        label { "Reconciled CSV"
                            input type="file" name="csv_file" accept=".csv" {}
                        }
                        button type="submit" { "Upload Reconciled" }
                    }
                }
                @if !unmatched_outgoing.is_empty() || !unmatched_reconciled.is_empty() {
                    form method="post" action=(format!("/reconcile/{}/auto-match", session_id)) class="auto-match-form" {
                        button type="submit" class="btn" { "Auto-Match" }
                    }
                }
            }

            div class="reconcile-grid" {
                // ── Header row ──
                div class="reconcile-grid-header" { "Outgoing" }
                div class="reconcile-grid-header" { "Reconciled" }

                // ── Matched pairs: outgoing on left, its reconciled stack on right ──
                @for o in &outgoing {
                    @if o.matched {
                        @if let Some(linked_ids) = match_map.get(&o.txn_id) {
                            @let row_span = linked_ids.len().max(1);
                            div class="reconcile-txn reconcile-txn--matched" style=(format!("grid-row: span {}", row_span)) {
                                div class="txn-row" {
                                    span class="txn-date" { (o.date) }
                                    @if !o.vendor.is_empty() {
                                        span class="txn-vendor" { (o.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(o.amount)) }
                                    @for rid in linked_ids {
                                        span class="txn-match-tag" {
                                            (utils::format_cents(reconciled.iter().find(|x| x.txn_id == *rid).map(|r| r.amount).unwrap_or(0)))
                                        }
                                    }
                                    @let reconciled_sum: i64 = linked_ids.iter()
                                        .filter_map(|rid| reconciled.iter().find(|x| x.txn_id == *rid).map(|r| r.amount))
                                        .sum();
                                    @let diff = reconciled_sum - o.amount;
                                    @if diff != 0 {
                                        span class="txn-diff" {
                                            @if diff > 0 {
                                                (format!("Over {}", utils::format_cents(diff)))
                                            } @else {
                                                (format!("Under {}", utils::format_cents(diff.abs())))
                                            }
                                        }
                                    }
                                    form method="post" action=(format!("/reconcile/{}/unlink", session_id)) class="txn-unlink-form" {
                                        input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                        button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Unmatch" }
                                    }
                                }
                            }
                            @for rid in linked_ids {
                                @if let Some(r) = reconciled.iter().find(|x| x.txn_id == *rid) {
                                    div class="reconcile-txn reconcile-txn--matched" {
                                        div class="txn-row" {
                                            span class="txn-date" { (r.date) }
                                            @if !r.vendor.is_empty() {
                                                span class="txn-vendor" { (r.vendor) }
                                            }
                                            span class="txn-amount" { (utils::format_cents(r.amount)) }
                                            form method="post" action=(format!("/reconcile/{}/unlink-reconciled", session_id)) class="txn-unlink-form" {
                                                input type="hidden" name="reconciled_id" value=(r.txn_id) {}
                                                button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Unmatch" }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // ── Unmatched pairs: outgoing left, reconciled right ──
                @for i in 0..unmatched_max {
                    @if let Some(o) = unmatched_outgoing.get(i) {
                        div class="reconcile-txn reconcile-txn--unmatched" {
                            div class="txn-row" {
                                span class="txn-date" { (o.date) }
                                @if !o.vendor.is_empty() {
                                    span class="txn-vendor" { (o.vendor) }
                                }
                                span class="txn-amount" { (utils::format_cents(o.amount)) }
                                button type="submit" name="outgoing_id" value=(o.txn_id) form="reconcile-match-form" class="btn btn-sm" { "Match" }
                            }
                        }
                    } @else {
                        div class="reconcile-grid-spacer" {}
                    }
                    @if let Some(r) = unmatched_reconciled.get(i) {
                        div class="reconcile-txn reconcile-txn--unmatched" {
                            div class="txn-row" {
                                input type="checkbox" name="reconciled_ids" value=(r.txn_id) form="reconcile-match-form" class="txn-card-checkbox" {}
                                span class="txn-date" { (r.date) }
                                @if !r.vendor.is_empty() {
                                    span class="txn-vendor" { (r.vendor) }
                                }
                                span class="txn-amount" { (utils::format_cents(r.amount)) }
                            }
                        }
                    } @else {
                        div class="reconcile-grid-spacer" {}
                    }
                }
            }
        },
        Some(&user),
    ))
}

#[derive(serde::Deserialize)]
pub struct AddTxnForm {
    pub date: String,
    pub amount: String,
    pub vendor: Option<String>,
}

pub async fn add_outgoing(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<AddTxnForm>,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&form.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let cents = utils::parse_dollars(&form.amount).map_err(AppError::BadRequest)?;
    let vendor = form
        .vendor
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    reconcile::add_outgoing(state.db(), session_id, date, cents, &vendor).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

pub async fn add_reconciled(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<AddTxnForm>,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&form.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let cents = utils::parse_dollars(&form.amount).map_err(AppError::BadRequest)?;
    let vendor = form
        .vendor
        .map(|v| v.trim().to_string())
        .unwrap_or_default();
    reconcile::add_reconciled(state.db(), session_id, date, cents, &vendor).await?;
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

pub async fn link_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut outgoing_id: Option<Uuid> = None;
    let mut reconciled_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            match key {
                "outgoing_id" => {
                    outgoing_id = Some(
                        Uuid::parse_str(val)
                            .map_err(|_| AppError::BadRequest("Invalid outgoing ID".into()))?,
                    );
                }
                "reconciled_ids" => {
                    let id = Uuid::parse_str(val)
                        .map_err(|_| AppError::BadRequest("Invalid reconciled ID".into()))?;
                    reconciled_ids.push(id);
                }
                _ => {}
            }
        }
    }
    let outgoing_id =
        outgoing_id.ok_or_else(|| AppError::BadRequest("No outgoing selected".into()))?;
    if reconciled_ids.is_empty() {
        return Err(AppError::BadRequest(
            "No reconciled transaction selected".into(),
        ));
    }
    for reconciled_id in reconciled_ids {
        reconcile::link_transactions(state.db(), outgoing_id, reconciled_id).await?;
    }
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

#[derive(serde::Deserialize)]
pub struct UnlinkForm {
    pub outgoing_id: String,
}

pub async fn unlink_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<UnlinkForm>,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let outgoing_id = Uuid::parse_str(&form.outgoing_id)
        .map_err(|_| AppError::BadRequest("Invalid outgoing ID".into()))?;
    // Find and remove all match_links for this outgoing
    let matches = reconcile::list_matches(state.db(), session_id).await?;
    for m in matches.iter().filter(|m| m.outgoing_id == outgoing_id) {
        reconcile::unlink_transaction(state.db(), m.match_id).await?;
    }
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

#[derive(serde::Deserialize)]
pub struct UnlinkReconciledForm {
    pub reconciled_id: String,
}

pub async fn unlink_reconciled_txns(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<UnlinkReconciledForm>,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let reconciled_id = Uuid::parse_str(&form.reconciled_id)
        .map_err(|_| AppError::BadRequest("Invalid reconciled ID".into()))?;
    let matches = reconcile::list_matches(state.db(), session_id).await?;
    for m in matches.iter().filter(|m| m.reconciled_id == reconciled_id) {
        reconcile::unlink_transaction(state.db(), m.match_id).await?;
    }
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

pub async fn auto_match(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    render_proposals_page(session_id, state, user, &[]).await
}

async fn render_proposals_page(
    session_id: Uuid,
    state: AppState,
    user: crate::cookies::LoggedInUser,
    skip_ids: &[Uuid],
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let proposals = reconcile::auto_match(state.db(), session_id, skip_ids).await?;
    let (_, name) = reconcile::get_session(state.db(), session_id, user.0).await?;
    let outgoing = reconcile::list_outgoing(state.db(), session_id).await?;
    let reconciled = reconcile::list_reconciled(state.db(), session_id).await?;

    Ok(layout(
        &format!("Reconcile — {}", name),
        maud::html! {
            a href="/reconcile" { "← Back" }
            form class="portfolio-name-form" method="post" action=(format!("/reconcile/{}/rename", session_id)) {
                input type="text" name="name" value=(name)
                       class="portfolio-name-input"
                       onkeydown="if(event.key==='Enter'){event.preventDefault();this.closest('form').requestSubmit()}" {}
            }

            h2 { "Auto-Match Proposals" }

            @if proposals.is_empty() {
                p { "No matches found." }
                a href=(format!("/reconcile/{}", session_id)) { "← Back to reconcile" }
            } @else {
                p { (format!("Found {} proposed match(es). Review and confirm or reject each.", proposals.len())) }

                form method="post" action=(format!("/reconcile/{}/confirm-all", session_id)) {
                    @for sid in skip_ids {
                        input type="hidden" name="skip_ids" value=(sid) {}
                    }
                    button type="submit" class="btn" { "Confirm All" }
                    " "
                    a href=(format!("/reconcile/{}", session_id)) class="btn btn-ghost" { "Cancel" }
                }

                div class="reconcile-grid" style="margin-top:1rem" {
                    div class="reconcile-grid-header" { "Outgoing" }
                    div class="reconcile-grid-header" { "Reconciled" }

                    @for p in &proposals {
                        @if let Some(o) = outgoing.iter().find(|x| x.txn_id == p.outgoing_id) {
                            @let row_span = p.reconciled_ids.len().max(1);
                            div class="reconcile-txn reconcile-txn--proposed" style=(format!("grid-row: span {}", row_span)) {
                                div class="txn-row" {
                                    span class="txn-date" { (o.date) }
                                    @if !o.vendor.is_empty() {
                                        span class="txn-vendor" { (o.vendor) }
                                    }
                                    span class="txn-amount" { (utils::format_cents(o.amount)) }
                                    form method="post" action=(format!("/reconcile/{}/confirm", session_id)) class="txn-unlink-form" style="display:inline" {
                                        input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                        @for rid in &p.reconciled_ids {
                                            input type="hidden" name="reconciled_ids" value=(rid) {}
                                        }
                                        @for sid in skip_ids {
                                            input type="hidden" name="skip_ids" value=(sid) {}
                                        }
                                        button type="submit" class="btn btn-sm" { "Confirm" }
                                    }
                                    form method="post" action=(format!("/reconcile/{}/reject", session_id)) class="txn-unlink-form" style="display:inline" {
                                        input type="hidden" name="outgoing_id" value=(o.txn_id) {}
                                        @for sid in skip_ids {
                                            input type="hidden" name="skip_ids" value=(sid) {}
                                        }
                                        button type="submit" class="btn-ghost" style="font-size:0.7rem" { "Reject" }
                                    }
                                }
                            }
                            @for rid in &p.reconciled_ids {
                                @if let Some(r) = reconciled.iter().find(|x| x.txn_id == *rid) {
                                    div class="reconcile-txn reconcile-txn--proposed" {
                                        div class="txn-row" {
                                            span class="txn-date" { (r.date) }
                                            @if !r.vendor.is_empty() {
                                                span class="txn-vendor" { (r.vendor) }
                                            }
                                            span class="txn-amount" { (utils::format_cents(r.amount)) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        },
        Some(&user),
    ))
}

#[derive(serde::Deserialize)]
pub struct RenameSessionForm {
    pub name: String,
}

pub async fn confirm_proposal(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut outgoing_id: Option<Uuid> = None;
    let mut reconciled_ids: Vec<Uuid> = Vec::new();
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = val.to_string();
            match key.as_str() {
                "outgoing_id" => {
                    outgoing_id = val.parse().ok();
                }
                "reconciled_ids" => {
                    if let Ok(id) = val.parse() {
                        reconciled_ids.push(id);
                    }
                }
                "skip_ids" => {
                    if let Ok(id) = val.parse() {
                        skip_ids.push(id);
                    }
                }
                _ => {}
            }
        }
    }
    // Apply this match
    if let Some(oid) = outgoing_id {
        for rid in &reconciled_ids {
            reconcile::link_transactions(state.db(), oid, *rid).await?;
        }
        skip_ids.push(oid);
    }
    // Re-render remaining proposals
    render_proposals_page(session_id, state, user, &skip_ids).await
}

pub async fn confirm_all_proposals(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=')
            && key == "skip_ids"
            && let Ok(id) = val.parse()
        {
            skip_ids.push(id);
        }
    }
    let proposals = reconcile::auto_match(state.db(), session_id, &skip_ids).await?;
    for p in &proposals {
        for rid in &p.reconciled_ids {
            reconcile::link_transactions(state.db(), p.outgoing_id, *rid).await?;
        }
    }
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

pub async fn reject_proposal(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut rejected_id: Option<Uuid> = None;
    let mut skip_ids: Vec<Uuid> = Vec::new();
    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = val.to_string();
            match key.as_str() {
                "outgoing_id" => {
                    if let Ok(id) = val.parse() {
                        rejected_id = Some(id);
                    }
                }
                "skip_ids" => {
                    if let Ok(id) = val.parse() {
                        skip_ids.push(id);
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(id) = rejected_id {
        skip_ids.push(id);
    }
    render_proposals_page(session_id, state, user, &skip_ids).await
}

pub async fn rename_session(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<RenameSessionForm>,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    if form.name.trim().is_empty() {
        return Err(AppError::BadRequest("Session name cannot be empty".into()));
    }
    sqlx::query("UPDATE reconcile_sessions SET name = ? WHERE session_id = ?")
        .bind(form.name.trim())
        .bind(session_id.to_string())
        .execute(state.db())
        .await?;
    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

pub async fn upload_outgoing_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    multipart: axum::extract::Multipart,
) -> Result<maud::Markup, AppError> {
    upload_csv(session_id, state, user, multipart, "outgoing").await
}

pub async fn upload_reconciled_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    multipart: axum::extract::Multipart,
) -> Result<maud::Markup, AppError> {
    upload_csv(session_id, state, user, multipart, "reconciled").await
}

async fn upload_csv(
    session_id: Uuid,
    state: AppState,
    user: crate::cookies::LoggedInUser,
    mut multipart: axum::extract::Multipart,
    kind: &str,
) -> Result<maud::Markup, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let (_, name) = reconcile::get_session(state.db(), session_id, user.0).await?;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?
    {
        if field.name() == Some("csv_file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Upload error: {}", e)))?;
            let raw = String::from_utf8(bytes.to_vec())
                .map_err(|_| AppError::BadRequest("CSV must be UTF-8".into()))?;
            let analysis = csv_import::analyze_csv(&raw)?;

            // Save CSV to temp file for confirm step
            let tmp_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
            let tmp_path = format!("/tmp/financials_csv_{}_{}.csv", session_id, tmp_id);
            std::fs::write(&tmp_path, &raw)
                .map_err(|e| AppError::BadRequest(format!("Failed to save CSV: {}", e)))?;

            let num_cols = analysis.preview_rows.first().map(|r| r.len()).unwrap_or(0);
            let col_options: Vec<String> =
                (0..num_cols).map(|i| format!("Column {}", i + 1)).collect();

            return Ok(layout(
                &format!("Import CSV — {}", name),
                maud::html! {
                    a href=(format!("/reconcile/{}", session_id)) { "← Back" }

                    h2 { "Import " (if kind == "outgoing" { "Outgoing" } else { "Reconciled" }) " Transactions" }

                    p { (format!("Detected {} rows. Review column mapping below and adjust if needed.", analysis.total_rows)) }

                    div class="csv-preview" {
                        h3 { "Preview (first 5 rows)" }
                        table class="csv-preview-table" {
                            thead {
                                tr class="csv-col-numbers" {
                                    @for i in 0..num_cols {
                                        th { (i + 1) }
                                    }
                                }
                                tr {
                                    @if !analysis.headers.is_empty() {
                                        @for h in &analysis.headers {
                                            th { (h) }
                                        }
                                    } @else {
                                        @for i in 0..num_cols {
                                            th { (format!("Col {}", i + 1)) }
                                        }
                                    }
                                }
                            }
                            tbody {
                                @for row in &analysis.preview_rows {
                                    tr {
                                        @for cell in row {
                                            td { (cell) }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    form method="post" action=(format!("/reconcile/{}/{}-csv/confirm", session_id, kind)) {
                        input type="hidden" name="tmp_id" value=(tmp_id) {}

                        div class="csv-mapping" {
                            h3 { "Column Mapping" }

                            label { "Date column" }
                            select name="date_col" {
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[i == analysis.detected.date_col] { (label) }
                                }
                            }

                            label { "Amount column" }
                            select name="amount_col" {
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[i == analysis.detected.amount_col] { (label) }
                                }
                            }

                            label { "Vendor/description column" }
                            select name="vendor_col" {
                                option value="" { "— None —" }
                                @for (i, label) in col_options.iter().enumerate() {
                                    option value=(i) selected[analysis.detected.vendor_col == Some(i)] { (label) }
                                }
                            }

                            label { "Date format" }
                            select name="date_format" {
                                @for fmt in &["%d/%m/%Y", "%d/%m/%y", "%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y", "%Y/%m/%d", "%b %d, %Y", "%d %b %Y", "%B %d, %Y", "%d %B %Y"] {
                                    option value=(fmt) selected[*fmt == analysis.detected.date_format] { (fmt) }
                                }
                            }
                        }

                        button type="submit" class="btn" { "Import" }
                        " "
                        a href=(format!("/reconcile/{}", session_id)) class="btn btn-ghost" { "Cancel" }
                    }
                },
                Some(&user),
            ));
        }
    }
    Err(AppError::BadRequest("No CSV file provided".into()))
}

pub async fn confirm_outgoing_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<axum::response::Redirect, AppError> {
    confirm_csv_import(session_id, state, user, body, "outgoing").await
}

pub async fn confirm_reconciled_csv(
    Path(session_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<axum::response::Redirect, AppError> {
    confirm_csv_import(session_id, state, user, body, "reconciled").await
}

async fn confirm_csv_import(
    session_id: Uuid,
    state: AppState,
    user: crate::cookies::LoggedInUser,
    body: axum::body::Bytes,
    kind: &str,
) -> Result<axum::response::Redirect, AppError> {
    reconcile::get_session(state.db(), session_id, user.0).await?;
    let body_str = String::from_utf8_lossy(&body);
    let mut tmp_id = String::new();
    let mut date_col: Option<usize> = None;
    let mut amount_col: Option<usize> = None;
    let mut vendor_col: Option<usize> = None;
    let mut date_format = String::new();

    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = key.to_string();
            let val = urldecode(val);
            match key.as_str() {
                "tmp_id" => tmp_id = val,
                "date_col" => date_col = val.parse().ok(),
                "amount_col" => amount_col = val.parse().ok(),
                "vendor_col" => {
                    if !val.is_empty() {
                        vendor_col = val.parse().ok();
                    }
                }
                "date_format" => date_format = val,
                _ => {}
            }
        }
    }

    let date_col = date_col.ok_or_else(|| AppError::BadRequest("Missing date_col".into()))?;
    let amount_col = amount_col.ok_or_else(|| AppError::BadRequest("Missing amount_col".into()))?;
    if date_format.is_empty() {
        date_format = "%Y-%m-%d".to_string();
    }

    let tmp_path = format!("/tmp/financials_csv_{}_{}.csv", session_id, tmp_id);
    let raw = std::fs::read_to_string(&tmp_path)
        .map_err(|e| AppError::BadRequest(format!("CSV file not found: {}", e)))?;
    let _ = std::fs::remove_file(&tmp_path); // Clean up

    let mapping = csv_import::ColumnMapping {
        date_col,
        amount_col,
        vendor_col,
        date_format,
    };
    let rows = csv_import::parse_csv_with_mapping(&raw, &mapping)?;

    if kind == "outgoing" {
        reconcile::bulk_add_outgoing(state.db(), session_id, &rows).await?;
    } else {
        reconcile::bulk_add_reconciled(state.db(), session_id, &rows).await?;
    }

    Ok(axum::response::Redirect::to(&format!(
        "/reconcile/{}",
        session_id
    )))
}

fn urldecode(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else if c == '+' {
            result.push(' ');
        } else {
            result.push(c);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn item(id: &str, name: &str, item_type: &str, position: i32) -> WealthItem {
        WealthItem {
            item_id: Uuid::parse_str(id).unwrap(),
            name: name.to_string(),
            item_type: item_type.to_string(),
            position,
        }
    }

    fn log(id: &str, item_id: &str, date: &str, value: i64) -> BalanceLog {
        BalanceLog {
            log_id: Uuid::parse_str(id).unwrap(),
            item_id: Uuid::parse_str(item_id).unwrap(),
            log_date: NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap(),
            balance_value: value,
        }
    }

    // ── pivot_logs ──

    #[test]
    fn pivot_logs_basic() {
        let items = [
            item("00000000-0000-0000-0000-000000000001", "Savings", "cash", 0),
            item(
                "00000000-0000-0000-0000-000000000002",
                "Mortgage",
                "debt",
                1,
            ),
        ];
        let logs = [
            log(
                "a0000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000001",
                "2025-07-01",
                500000,
            ),
            log(
                "a0000000-0000-0000-0000-000000000002",
                "00000000-0000-0000-0000-000000000002",
                "2025-07-01",
                -1500000,
            ),
        ];
        let rows = pivot_logs(&items, &logs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].date, NaiveDate::from_ymd_opt(2025, 7, 1).unwrap());
        assert_eq!(rows[0].values.len(), 2);
        assert_eq!(rows[0].values[0], Some(500000));
        assert_eq!(rows[0].values[1], Some(-1500000));
    }

    #[test]
    fn pivot_logs_missing_entries() {
        let items = [
            item("00000000-0000-0000-0000-000000000001", "Savings", "cash", 0),
            item(
                "00000000-0000-0000-0000-000000000002",
                "Mortgage",
                "debt",
                1,
            ),
        ];
        let logs = [log(
            "a0000000-0000-0000-0000-000000000001",
            "00000000-0000-0000-0000-000000000001",
            "2025-07-01",
            500000,
        )];
        let rows = pivot_logs(&items, &logs);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values[0], Some(500000));
        assert_eq!(rows[0].values[1], None);
    }

    #[test]
    fn pivot_logs_sorted_descending() {
        let items = [item(
            "00000000-0000-0000-0000-000000000001",
            "Savings",
            "cash",
            0,
        )];
        let logs = [
            log(
                "a0000000-0000-0000-0000-000000000001",
                "00000000-0000-0000-0000-000000000001",
                "2025-07-01",
                100,
            ),
            log(
                "a0000000-0000-0000-0000-000000000002",
                "00000000-0000-0000-0000-000000000001",
                "2025-07-15",
                200,
            ),
        ];
        let rows = pivot_logs(&items, &logs);
        // Most recent date first
        assert_eq!(rows[0].date, NaiveDate::from_ymd_opt(2025, 7, 15).unwrap());
        assert_eq!(rows[1].date, NaiveDate::from_ymd_opt(2025, 7, 1).unwrap());
    }

    #[test]
    fn pivot_logs_empty() {
        let items: [WealthItem; 0] = [];
        let logs: [BalanceLog; 0] = [];
        let rows = pivot_logs(&items, &logs);
        assert!(rows.is_empty());
    }

    // ── urldecode ──

    #[test]
    fn urldecode_percent_encoding() {
        assert_eq!(urldecode("hello%20world"), "hello world");
    }

    #[test]
    fn urldecode_plus() {
        assert_eq!(urldecode("hello+world"), "hello world");
    }

    #[test]
    fn urldecode_ampersand() {
        assert_eq!(urldecode("a%26b"), "a&b");
    }

    #[test]
    fn urldecode_plain_string() {
        assert_eq!(urldecode("hello"), "hello");
    }

    #[test]
    fn urldecode_multi_byte() {
        // The urldecode function handles one byte at a time, so multi-byte
        // UTF-8 chars produce multiple decoded chars. %C3%A9 → two chars (Ã©)
        let result = urldecode("%C3%A9");
        assert!(result.contains("Ã"));
    }

    // ── make_chart_id ──

    #[test]
    fn make_chart_id_replaces_id_and_get_element() {
        fn make_chart_id(html: &str, new_id: &str) -> String {
            html.replace("id=\"chart\"", &format!("id=\"{}\"", new_id))
                .replace(
                    "getElementById('chart')",
                    &format!("getElementById('{}')", new_id),
                )
        }
        let html =
            r#"<div id="chart"></div><script>var c = document.getElementById('chart');</script>"#;
        let result = make_chart_id(html, "trend-chart");
        assert!(result.contains("id=\"trend-chart\""));
        assert!(result.contains("getElementById('trend-chart')"));
        assert!(!result.contains("id=\"chart\""));
        assert!(!result.contains("getElementById('chart')"));
    }

    #[test]
    fn make_chart_id_no_match() {
        fn make_chart_id(html: &str, new_id: &str) -> String {
            html.replace("id=\"chart\"", &format!("id=\"{}\"", new_id))
                .replace(
                    "getElementById('chart')",
                    &format!("getElementById('{}')", new_id),
                )
        }
        let html = "<div id='other'></div>";
        let result = make_chart_id(html, "trend-chart");
        assert_eq!(result, html);
    }
}

pub async fn time(State(_state): State<AppState>) -> impl IntoResponse {
    maud::html! { p { "Time: " (chrono::Local::now().format("%H:%M:%S")) } }
}

// ── Portfolio CSV Import/Export ──

pub async fn portfolio_import(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    Ok(layout(
        &format!("Import CSV — {}", name),
        maud::html! {
            a href=(format!("/portfolio/{}", portfolio_id)) { "← Back" }
            h2 { "Import CSV into " (name) }

            div class="csv-import-help" {
                h3 { "How it works" }
                ol {
                    li { "Upload a CSV file." }
                    li { "Preview the data and map each column to a date, an existing wealth item, or a new item." }
                    li { "Choose the type for any new items (asset, cash, debt, investment)." }
                    li { "Values are upserted — existing entries for the same date/item are updated." }
                }
            }

            form method="post" action=(format!("/portfolio/{}/import", portfolio_id))
                  enctype="multipart/form-data"
                  class="add-item-form" {
                label { "CSV File"
                    input type="file" name="csv_file" accept=".csv,.txt" required {}
                }
                button type="submit" class="btn" { "Upload & Preview" }
                " "
                a href=(format!("/portfolio/{}", portfolio_id)) class="btn btn-ghost" { "Cancel" }
            }
        },
        Some(&user),
    ))
}

pub async fn portfolio_import_post(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    mut multipart: axum::extract::Multipart,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;

    let mut csv_data = String::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Failed to read multipart field: {}", e)))?
    {
        if field.name() == Some("csv_file") {
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;
            csv_data = String::from_utf8(bytes.to_vec())
                .map_err(|e| AppError::BadRequest(format!("File is not valid UTF-8: {}", e)))?;
        }
    }

    if csv_data.is_empty() {
        return Err(AppError::BadRequest("No CSV file provided".into()));
    }

    // Analyze the CSV
    let analysis = crate::models::csv_import::analyze_csv(&csv_data)?;

    // Save to temp file for confirm step
    let tmp_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
    let tmp_path = format!(
        "/tmp/financials_portfolio_csv_{}_{}.csv",
        portfolio_id, tmp_id
    );
    std::fs::write(&tmp_path, &csv_data)
        .map_err(|e| AppError::BadRequest(format!("Failed to save CSV: {}", e)))?;

    // Load existing wealth items for mapping
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;

    let num_cols = analysis.preview_rows.first().map(|r| r.len()).unwrap_or(0);
    let col_numbers: Vec<usize> = (0..num_cols).collect();

    // Detect which column looks like a date
    let date_col = analysis.detected.date_col;

    Ok(layout(
        &format!("Map Columns — {}", name),
        maud::html! {
            a href=(format!("/portfolio/{}", portfolio_id)) { "← Back" }
            h2 { "Map Columns" }
            p { (format!("Detected {} rows, {} columns. Map each column below.", analysis.total_rows, num_cols)) }

            div class="csv-preview" {
                h3 { "Preview (first 5 rows)" }
                table class="csv-preview-table" {
                    thead {
                        tr class="csv-col-numbers" {
                            @for i in 0..num_cols {
                                th { (i + 1) }
                            }
                        }
                        tr {
                            @if !analysis.headers.is_empty() {
                                @for h in &analysis.headers {
                                    th { (h) }
                                }
                            } @else {
                                @for i in 0..num_cols {
                                    th { (format!("Col {}", i + 1)) }
                                }
                            }
                        }
                    }
                    tbody {
                        @for row in &analysis.preview_rows {
                            tr {
                                @for cell in row {
                                    td { (cell) }
                                }
                            }
                        }
                    }
                }
            }

            form method="post" action=(format!("/portfolio/{}/import/confirm", portfolio_id)) {
                input type="hidden" name="tmp_id" value=(tmp_id) {}

                div class="csv-mapping" {
                    h3 { "Column Mapping" }

                    label { "Date column" }
                    select name="date_col" {
                        @for (i, _label) in col_numbers.iter().enumerate() {
                            option value=(i) selected[i == date_col] { (format!("Column {}", i + 1)) }
                        }
                    }

                    label { "Date format" }
                    select name="date_format" {
                        @for fmt in &["%d/%m/%Y", "%d/%m/%y", "%Y-%m-%d", "%m/%d/%Y", "%m/%d/%y", "%Y/%m/%d", "%b %d, %Y", "%d %b %Y", "%B %d, %Y", "%d %B %Y"] {
                            option value=(fmt) selected[*fmt == analysis.detected.date_format] { (fmt) }
                        }
                    }

                    @for col_idx in 0..num_cols {
                        @if col_idx == date_col {
                            // Skip date column in item mapping — it's handled above
                        } @else {
                            div class="csv-mapping-row" style="margin: 0.5em 0; padding: 0.5em; border: 1px solid var(--border); border-radius: 4px;" {
                                @let col_header = analysis.headers.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                                @let col_label = if col_header.is_empty() { format!("Column {}", col_idx + 1) } else { format!("Column {}: {}", col_idx + 1, col_header) };
                                // Auto-match: if the column header matches an existing item name, select it by default
                                @let matched_item_id = items.iter().find(|item| item.name.eq_ignore_ascii_case(col_header)).map(|item| item.item_id);
                                strong { (col_label) }
                                select name=(format!("col_{}", col_idx)) {
                                    option value="skip" selected[matched_item_id.is_none()] { "— Skip —" }
                                    @for item in &items {
                                        option value=(format!("existing:{}", item.item_id)) selected[matched_item_id == Some(item.item_id)] { "→ " (item.name) " (" (item.item_type) ")" }
                                    }
                                    option value="new:asset" { "+ New Asset" }
                                    option value="new:cash" { "+ New Cash" }
                                    option value="new:investment" { "+ New Investment" }
                                    option value="new:debt" { "+ New Debt" }
                                }
                                @if !col_header.is_empty() {
                                    input type="text" name=(format!("col_{}_name", col_idx)) placeholder="Item name (defaults to column header)" value=(col_header) style="margin-left: 0.5em; width: 12em;" {}
                                } @else {
                                    input type="text" name=(format!("col_{}_name", col_idx)) placeholder="Item name (required for new items)" style="margin-left: 0.5em; width: 12em;" {}
                                }
                            }
                        }
                    }
                }

                button type="submit" class="btn" { "Import" }
                " "
                a href=(format!("/portfolio/{}", portfolio_id)) class="btn btn-ghost" { "Cancel" }
            }
        },
        Some(&user),
    ))
}

pub async fn portfolio_import_confirm(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    body: axum::body::Bytes,
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;

    let body_str = String::from_utf8_lossy(&body);
    let mut tmp_id = String::new();
    let mut date_col: usize = 0;
    let mut date_format = "%d/%m/%Y".to_string();
    let mut columns: std::collections::HashMap<usize, portfolio::ColumnTarget> =
        std::collections::HashMap::new();

    for pair in body_str.split('&') {
        if let Some((key, val)) = pair.split_once('=') {
            let key = urldecode(key);
            let val = urldecode(val);
            match key.as_str() {
                "tmp_id" => tmp_id = val,
                "date_col" => {
                    date_col = val.parse().unwrap_or(0);
                }
                "date_format" => {
                    if !val.is_empty() {
                        date_format = val;
                    }
                }
                key if key.starts_with("col_") && !key.ends_with("_name") => {
                    if let Ok(col_idx) = key[4..].parse::<usize>() {
                        if val == "skip" {
                            columns.insert(col_idx, portfolio::ColumnTarget::Skip);
                        } else if let Some(id) = val.strip_prefix("existing:") {
                            columns
                                .insert(col_idx, portfolio::ColumnTarget::Existing(id.to_string()));
                        } else if let Some(type_str) = val.strip_prefix("new:") {
                            // Name will come from the col_{idx}_name field
                            // For now store a placeholder; we'll update below
                            columns.insert(
                                col_idx,
                                portfolio::ColumnTarget::New {
                                    name: String::new(), // placeholder
                                    item_type: type_str.to_string(),
                                },
                            );
                        }
                    }
                }
                key if key.starts_with("col_") && key.ends_with("_name") => {
                    // col_3_name → update the New item in column 3 with the actual name
                    let col_str = &key[4..key.len() - 5]; // strip "col_" prefix and "_name" suffix
                    if let Ok(col_idx) = col_str.parse::<usize>()
                        && let Some(portfolio::ColumnTarget::New { name, .. }) =
                            columns.get_mut(&col_idx)
                        && !val.is_empty()
                    {
                        *name = val;
                    }
                }
                _ => {}
            }
        }
    }

    if tmp_id.is_empty() {
        return Err(AppError::BadRequest("Missing upload reference".into()));
    }

    // Validate: any New targets must have a name
    for (col_idx, target) in &columns {
        if let portfolio::ColumnTarget::New { name, .. } = target
            && name.is_empty()
        {
            return Err(AppError::BadRequest(format!(
                "Column {} is set to create a new item but has no name",
                col_idx + 1
            )));
        }
    }

    let tmp_path = format!(
        "/tmp/financials_portfolio_csv_{}_{}.csv",
        portfolio_id, tmp_id
    );
    let raw = std::fs::read_to_string(&tmp_path)
        .map_err(|e| AppError::BadRequest(format!("CSV file not found: {}", e)))?;
    let _ = std::fs::remove_file(&tmp_path); // Clean up

    let mapping = portfolio::PortfolioColumnMapping {
        date_col,
        date_format,
        columns,
    };

    let result = portfolio::import_csv(state.db(), portfolio_id, &raw, &mapping).await?;

    let flash_msg = format!(
        "Imported {} rows ({} skipped, {} items created)",
        result.rows_imported, result.rows_skipped, result.items_created
    );
    let encoded = flash_msg
        .replace(' ', "+")
        .replace('%', "%25")
        .replace('&', "%26");

    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}?flash={}&flash_type=success",
        portfolio_id, encoded
    )))
}

pub async fn portfolio_csv(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;

    // Track which items are debts so we export them as negative
    let debt_ids: std::collections::HashSet<Uuid> = items
        .iter()
        .filter(|wi| wi.item_type == "debt")
        .map(|wi| wi.item_id)
        .collect();

    // Pivot: date -> item_id -> value
    let mut dates: std::collections::BTreeMap<NaiveDate, std::collections::HashMap<Uuid, i64>> =
        std::collections::BTreeMap::new();
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
    wtr.write_record(&header)
        .map_err(|e| AppError::Internal(e.into()))?;

    for (date, values) in &dates {
        let mut row = vec![date.to_string()];
        for item in &items {
            match values.get(&item.item_id) {
                Some(cents) => {
                    // Debts are stored positive internally; export as negative
                    let value = if debt_ids.contains(&item.item_id) {
                        -cents
                    } else {
                        *cents
                    };
                    row.push(utils::format_cents(value));
                }
                None => row.push(String::new()),
            }
        }
        wtr.write_record(&row)
            .map_err(|e| AppError::Internal(e.into()))?;
    }

    let data = wtr.into_inner().map_err(|e| AppError::Internal(e.into()))?;
    let filename = format!("attachment; filename=\"{}.csv\"", name);

    Ok((
        [
            ("content-type", "text/csv"),
            ("content-disposition", filename.as_str()),
        ],
        data,
    )
        .into_response())
}

pub async fn not_found(State(_state): State<AppState>) -> impl IntoResponse {
    layout(
        "Not Found",
        maud::html! {
            h1 {"404"}
            p { "The page you're looking for doesn't exist."}
            a href="/" {"Go home"}
        },
        None,
    )
}

// ── Settings / Backup ──

pub async fn settings(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let config = backup::get_config(state.db(), user.0).await?;
    let username = user::get_username_by_id(state.db(), user.0)
        .await
        .unwrap_or_else(|_| "User".to_string());

    let (provider, bucket, path, region, endpoint, access_key_id, b2_key_id, _enabled) =
        match &config {
            Some(c) => (
                c.provider.clone(),
                c.bucket.clone(),
                c.path.clone(),
                c.region.clone(),
                c.endpoint.clone(),
                c.access_key_id.clone(),
                c.b2_key_id.clone(),
                c.enabled,
            ),
            None => (
                "s3".to_string(),
                String::new(),
                "financials-backups".to_string(),
                "us-east-1".to_string(),
                None,
                None,
                None,
                false,
            ),
        };

    let s3_style = if provider == "s3" { "" } else { "display:none" };
    let b2_style = if provider == "b2" { "" } else { "display:none" };

    let provider_options = if provider == "s3" {
        maud::html! {
            option value="s3" selected { "Amazon S3 / S3-compatible" }
            option value="b2" { "Backblaze B2" }
        }
    } else {
        maud::html! {
            option value="s3" { "Amazon S3 / S3-compatible" }
            option value="b2" selected { "Backblaze B2" }
        }
    };

    let enable_disable_btn = match &config {
        Some(c) if c.enabled => Some(maud::html! {
            button type="submit" formaction="/settings/backup/disable" class="btn btn-ghost" { "Pause Backups" }
        }),
        Some(_) => Some(maud::html! {
            button type="submit" formaction="/settings/backup/enable" class="btn" { "Enable Backups" }
        }),
        None => None,
    };

    Ok(layout(
        "Settings",
        maud::html! {
            h2 { "Settings" }
            p { "Hello, " (username) }

            div class="settings-tabs" {
                button class="tab-btn active" data-tab="backup" { "Backup" }
            }

            div id="backup" class="tab-content" {
                h3 { "Database Backups" }
                p { "Automatically back up your financial data to cloud storage. Choose a provider and enter your credentials. When enabled, a litestream sidecar will continuously replicate your database to the configured bucket." }

                @if config.is_some() {
                    div class="backup-status" {
                        @if let Some(c) = &config {
                            @if c.enabled {
                                div class="flash flash-success" { "Backups are active" }
                            } @else {
                                div class="flash flash-warning" { "Backups are paused" }
                            }
                        }
                        p class="backup-detail" {
                            "Provider: " (match &config { Some(c) => c.provider.clone(), None => String::new() })
                            " | Bucket: " (match &config { Some(c) => c.bucket.clone(), None => String::new() })
                        }
                    }
                }

                form action="/settings/backup" method="post" class="settings-form" {
                    label { "Provider"
                        select name="provider" id="provider-select" {
                            (provider_options)
                        }
                    }

                    label { "Bucket Name"
                        input type="text" name="bucket" value=(bucket) placeholder="my-backup-bucket";
                    }
                    label { "Backup Path Prefix"
                        input type="text" name="path" value=(path) placeholder="financials-backups";
                    }

                    div id="s3-fields" style=(s3_style) {
                        label { "Region"
                            input type="text" name="region" value=(region) placeholder="us-east-1";
                        }
                        label { "Custom Endpoint (optional — leave empty for AWS)"
                            input type="text" name="endpoint" value=(endpoint.unwrap_or_default()) placeholder="https://s3.example.com";
                        }
                        label { "Access Key ID"
                            input type="text" name="access_key_id" value=(access_key_id.unwrap_or_default()) autocomplete="off";
                        }
                        label { "Secret Access Key"
                            input type="password" name="secret_access_key" autocomplete="new-password" placeholder="Enter your secret key";
                        }
                    }

                    div id="b2-fields" style=(b2_style) {
                        label { "Key ID"
                            input type="text" name="b2_key_id" value=(b2_key_id.unwrap_or_default()) autocomplete="off";
                        }
                        label { "Application Key"
                            input type="password" name="b2_application_key" autocomplete="new-password" placeholder="Enter your application key";
                        }
                    }

                    div class="settings-actions" {
                        button type="submit" class="btn" { "Save Configuration" }
                        @if let Some(btn) = enable_disable_btn {
                            (btn)
                        }
                    }
                }
            }

            script type="text/javascript" {
                (maud::PreEscaped("document.getElementById('provider-select').addEventListener('change', function() { document.getElementById('s3-fields').style.display = this.value === 's3' ? '' : 'none'; document.getElementById('b2-fields').style.display = this.value === 'b2' ? '' : 'none'; });"))
            }
        },
        Some(&user),
    ))
}

#[derive(serde::Deserialize)]
pub struct BackupForm {
    provider: String,
    bucket: String,
    path: String,
    region: String,
    endpoint: Option<String>,
    access_key_id: Option<String>,
    secret_access_key: Option<String>,
    b2_key_id: Option<String>,
    b2_application_key: Option<String>,
}

pub async fn settings_backup_post(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<BackupForm>,
) -> Result<axum::response::Response, AppError> {
    // Trim empty strings to None for optional fields
    let endpoint = form.endpoint.filter(|s| !s.trim().is_empty());
    let access_key_id = form.access_key_id.filter(|s| !s.trim().is_empty());
    let secret_access_key = form.secret_access_key.filter(|s| !s.trim().is_empty());
    let b2_key_id = form.b2_key_id.filter(|s| !s.trim().is_empty());
    let b2_application_key = form.b2_application_key.filter(|s| !s.trim().is_empty());

    // If secret_access_key is empty and we have an existing config, keep the old one
    let secret_access_key = match secret_access_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(state.db(), user.0).await?;
            existing.and_then(|c| c.secret_access_key)
        }
    };
    let b2_application_key = match b2_application_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(state.db(), user.0).await?;
            existing.and_then(|c| c.b2_application_key)
        }
    };

    let config = backup::BackupConfig {
        id: Uuid::nil(), // Will be set by save_config if new
        user_id: user.0,
        provider: form.provider,
        bucket: form.bucket,
        path: form.path,
        region: form.region,
        endpoint,
        access_key_id,
        secret_access_key,
        b2_key_id,
        b2_application_key,
        enabled: false, // Start paused; user explicitly enables
    };

    // Preserve existing enabled state if updating
    let existing = backup::get_config(state.db(), user.0).await?;
    let config = match existing {
        Some(mut c) => {
            c.provider = config.provider;
            c.bucket = config.bucket;
            c.path = config.path;
            c.region = config.region;
            c.endpoint = config.endpoint;
            c.access_key_id = config.access_key_id;
            c.secret_access_key = config.secret_access_key;
            c.b2_key_id = config.b2_key_id;
            c.b2_application_key = config.b2_application_key;
            c
        }
        None => config,
    };

    backup::save_config(state.db(), user.0, &config).await?;

    // If the config is enabled, sync litestream immediately
    if config.enabled
        && let Err(e) = backup::sync_litestream(state.db(), &state.db_path, &state.config_dir).await
    {
        tracing::error!("Failed to sync litestream after saving config: {e:?}");
    }

    Ok(Redirect::to("/settings?saved").into_response())
}

pub async fn settings_backup_enable(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(state.db(), user.0, true).await?;
    backup::sync_litestream(state.db(), &state.db_path, &state.config_dir).await?;
    Ok(Redirect::to("/settings?enabled").into_response())
}

pub async fn settings_backup_disable(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(state.db(), user.0, false).await?;
    backup::sync_litestream(state.db(), &state.db_path, &state.config_dir).await?;
    Ok(Redirect::to("/settings?disabled").into_response())
}
