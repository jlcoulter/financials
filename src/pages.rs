use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::portfolio::{self, WealthItem, BalanceLog};
use crate::utils;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
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

pub async fn portfolios(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(state.db(), user.0).await?;
    Ok(layout(
        "Portfolios",
        maud::html! {
            details {
                summary { "+ New Portfolio" }
                form method="post" action="/portfolios" {
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

pub async fn portfolio(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;
    let grid_rows = pivot_logs(&items, &logs);

    Ok(layout(
        &format!("portfolio - {}", name),
        maud::html! {
            a href="/portfolios" { "<- Back" }
            h2 { (name) }

            details {
                summary { "+ Add Wealth Item"}
                form method="post" action=(format!("/portfolio/{}/items", portfolio_id)) {
                    label { "Name"
                    input type="text" name="name" required {}
            }
            label {"Type"
        select name="item_type" {
            option value="asset" {"Asset"}
            option value="debt" {"Debt"}
            option value="investment" {"Investment"}
        }
        }
            button type="submit" {"Add Item"}
                }
            }
            @if items.is_empty() {
                p { "No wealth items yet. Add one to start tracking." }
            }
                @else {
                ul {
                    @for item in &items {
                        li { (item.name) " - " (item.item_type) }
                    }
                }
            }
            @if !items.is_empty() {
                div class="grid-wrapper" {
                    table {
                        thead {
                            tr {
                                th { "Date" }
                                @for item in &items {
                                    th id=(format!("th-{}", item.item_id)) class="editable"
                                       tabindex="0"
                                       hx-get=(format!("/portfolio/{}/rename-item?item_id={}", portfolio_id, item.item_id))
                                       hx-target=(format!("#th-{}", item.item_id))
                                       hx-swap="outerHTML" {
                                        (item.name)
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
                                    .filter_map(|(i, v)| match v {
                                        Some(val) => Some(if items[i].item_type == "debt" { -*val } else { *val }),
                                        None => None,
                                    })
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
        if let Some(value) = form.get(&key) {
            if let Ok(cents) = utils::parse_dollars(value) {
                portfolio::insert_balance_log(state.db(), item.item_id, log_date, cents).await?;
            }
        }
    }

    // Build the values for this date
    let item_index: std::collections::HashMap<Uuid, usize> = items
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

    let total: i64 = values.iter().enumerate()
        .filter_map(|(i, v)| match v {
            Some(val) => Some(if items[i].item_type == "debt" { -*val } else { *val }),
            None => None,
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
pub struct ItemQuery {
    item_id: String,
}

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
    let current_cents = logs.iter()
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

    let cancel_url = format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date);
    let target_sel = format!("#{}", cell_id);

    Ok(maud::html! {
        td id=(cell_id) class="editable" tabindex="0"
           hx-get=(format!("/portfolio/{}/cell?item_id={}&date={}", portfolio_id, item_id, date))
           hx-target=(format!("#{}", cell_id))
           hx-swap="outerHTML" {
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
                       hx-on--blur="this.closest('form').requestSubmit()"
                       hx-on--keydown=(format!("if(event.key==='Enter'){{event.preventDefault();this.closest('form').requestSubmit()}}else if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','{}',{{target:'{}',swap:'outerHTML'}})}}", cancel_url, target_sel))
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
    let item_id_str = form.get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = Uuid::parse_str(item_id_str)
        .map_err(|_| AppError::BadRequest("Invalid item ID.".into()))?;
    let date_str = form.get("date")
        .ok_or_else(|| AppError::BadRequest("Missing date".into()))?;
    let date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let value_str = form.get("value")
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

    let cents = utils::parse_dollars(value_str)
        .map_err(AppError::BadRequest)?;
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
                       hx-on--blur="this.closest('form').requestSubmit()"
                       hx-on--keydown=(format!("if(event.key==='Enter'){{event.preventDefault();this.closest('form').requestSubmit()}}else if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','/portfolio/{}/row?date={}',{{target:'{}',swap:'outerHTML'}})}}", portfolio_id, date, format!("#row-{}", date)))
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
    let total: i64 = values.iter().enumerate()
        .filter_map(|(i, v)| match v {
            Some(val) => Some(if items[i].item_type == "debt" { -*val } else { *val }),
            None => None,
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
    let old_date_str = form.get("old_date")
        .ok_or_else(|| AppError::BadRequest("Missing old_date".into()))?;
    let new_date_str = form.get("new_date")
        .ok_or_else(|| AppError::BadRequest("Missing new_date".into()))?;
    let old_date = NaiveDate::parse_from_str(old_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid old date format. Use YYYY-MM-DD.".into()))?;

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let logs = portfolio::list_balance_logs(state.db(), portfolio_id).await?;

    // If new_date is invalid, re-render the original row with an error
    let new_date = match NaiveDate::parse_from_str(new_date_str, "%Y-%m-%d") {
        Ok(d) => d,
        Err(_) => {
            let values: Vec<Option<i64>> = items.iter().map(|item| {
                logs.iter()
                    .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                    .map(|l| l.balance_value)
            }).collect();
            return Ok(render_data_row_with_error(
                portfolio_id, &items, old_date, &values,
                "Invalid date format. Use YYYY-MM-DD.",
            ));
        }
    };

    if old_date == new_date {
        let values: Vec<Option<i64>> = items.iter().map(|item| {
            logs.iter()
                .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                .map(|l| l.balance_value)
        }).collect();
        return Ok(render_data_row(portfolio_id, &items, old_date, &values));
    }

    match portfolio::rename_date(state.db(), portfolio_id, old_date, new_date).await {
        Ok(_) => {}
        Err(AppError::BadRequest(msg)) => {
            let values: Vec<Option<i64>> = items.iter().map(|item| {
                logs.iter()
                    .find(|l| l.item_id == item.item_id && l.log_date == old_date)
                    .map(|l| l.balance_value)
            }).collect();
            return Ok(render_data_row_with_error(portfolio_id, &items, old_date, &values, &msg));
        }
        Err(e) => return Err(e),
    }

    let values: Vec<Option<i64>> = items.iter().map(|item| {
        logs.iter()
            .find(|l| l.item_id == item.item_id && l.log_date == new_date)
            .map(|l| l.balance_value)
    }).collect();

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
    let values: Vec<Option<i64>> = items.iter().map(|item| {
        logs.iter()
            .find(|l| l.item_id == item.item_id && l.log_date == date)
            .map(|l| l.balance_value)
    }).collect();

    Ok(render_data_row(portfolio_id, &items, date, &values))
}

pub async fn edit_item_name(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<ItemQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let item_id = Uuid::parse_str(&query.item_id)?;

    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    let item = items.iter().find(|i| i.item_id == item_id)
        .ok_or_else(|| AppError::BadRequest("Item not found".into()))?;

    let th_id = format!("th-{}", item_id);
    let cancel_url = format!("/portfolio/{}/rename-item?item_id={}", portfolio_id, item_id);
    let target_sel = format!("#{}", th_id);

    Ok(maud::html! {
        th id=(th_id) class="editable" tabindex="0"
           hx-get=(cancel_url)
           hx-target=(target_sel)
           hx-swap="outerHTML" {
            form class="cell-edit-form"
                  hx-put=(format!("/portfolio/{}/rename-item", portfolio_id))
                  hx-target=(format!("#{}", th_id))
                  hx-swap="outerHTML"
                  hx-trigger="submit" {
                input type="hidden" name="item_id" value=(item_id) {}
                input type="text" name="name"
                       value=(item.name)
                       class="cell-edit-input"
                       hx-on--blur="this.closest('form').requestSubmit()"
                       hx-on--keydown=(format!("if(event.key==='Enter'){{event.preventDefault();this.closest('form').requestSubmit()}}else if(event.key==='Escape'){{event.preventDefault();htmx.ajax('GET','{}',{{target:'{}',swap:'outerHTML'}})}}", cancel_url, target_sel))
                       autofocus {}
            }
        }
    })
}

pub async fn save_item_name(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::Form(form): axum::Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;
    let item_id_str = form.get("item_id")
        .ok_or_else(|| AppError::BadRequest("Missing item_id".into()))?;
    let item_id = Uuid::parse_str(item_id_str)?;
    let name = form.get("name")
        .ok_or_else(|| AppError::BadRequest("Missing name".into()))?;

    if name.trim().is_empty() {
        return Err(AppError::BadRequest("Item name cannot be empty".into()));
    }

    portfolio::rename_wealth_item(state.db(), item_id, name.trim()).await?;

    let th_id = format!("th-{}", item_id);

    Ok(maud::html! {
        th id=(th_id) class="editable" tabindex="0"
           hx-get=(format!("/portfolio/{}/rename-item?item_id={}", portfolio_id, item_id))
           hx-target=(format!("#{}", th_id))
           hx-swap="outerHTML" {
            (name.trim())
        }
    })
}

pub async fn dashboard(user: LoggedInUser) -> impl IntoResponse {
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
            h2 { (greeting) ", " (user.0) }
            div class="cards"{
            div class="card" {
                h3 {"Your Account"}
                p { "Manage your profile and settings" }
            }
                div class="card" {
                h3 {"Activity"}
                p {"View your recent activity"}
            }
            }
        },
        Some(&user),
    )
}

pub async fn time(State(_state): State<AppState>) -> impl IntoResponse {
    maud::html! { p { "Time: " (chrono::Local::now().format("%H:%M:%S")) } }
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
