use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::csv_import;
use crate::models::portfolio::{self, WealthItem};
use crate::requests::{CellQuery, DateQuery};
use crate::utils;
use axum::extract::{Form, Multipart, Path, Query, State};
use axum::response::{IntoResponse, Redirect};
use chrono::NaiveDate;
use uuid::Uuid;

// ── Balance add (HTMX) ──

pub async fn add_balance(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;

    let log_date_str = form
        .get("log_date")
        .ok_or_else(|| AppError::BadRequest("Missing log date field".into()))?;
    let log_date = NaiveDate::parse_from_str(log_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;
    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    for item in &items {
        let key = format!("balance_{}", item.item_id);
        if let Some(value) = form.get(&key)
            && let Ok(cents) = utils::parse_dollars(value)
        {
            portfolio::insert_balance_log(&state.db().await, item.item_id, log_date, cents).await?;
        }
    }

    // Build the values for this date
    let _item_index: std::collections::HashMap<Uuid, usize> = items
        .iter()
        .enumerate()
        .map(|(i, wi)| (wi.item_id, i))
        .collect();

    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;
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

    // Return the new data row, plus OOB swap to reset the blank input row
    Ok(maud::html! {
        tr id=(format!("row-{}", log_date)) {
            td id=(format!("date-{}", log_date)) class="editable date-cell" tabindex="0"
               hx-get=(format!("/portfolio/{}/date?date={}", portfolio_id, log_date))
               hx-target=(format!("#date-{}", log_date))
               hx-swap="outerHTML" {
                (utils::format_date(log_date))
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
        // OOB swap: replace the blank row with a fresh one (clears inputs)
        tr id="blank-row" class="blank-row" hx-swap-oob="true" {
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
    })
}

// ── Inline cell editing (HTMX) ──

/// GET: return an inline form to edit one cell.
pub async fn edit_cell(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(query): Query<CellQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let item_id = Uuid::parse_str(&query.item_id)
        .map_err(|_| AppError::BadRequest("Invalid item ID.".into()))?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;

    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;
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
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
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
    portfolio::upsert_balance_log(&state.db().await, item_id, date, cents).await?;

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

pub async fn edit_date(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(query): Query<DateQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
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

// ── Data row rendering helpers ──

fn render_data_row(
    portfolio_id: Uuid,
    items: &[WealthItem],
    date: NaiveDate,
    values: &[Option<i64>],
) -> maud::Markup {
    render_data_row_inner(portfolio_id, items, date, values, None)
}

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
    Form(form): Form<std::collections::HashMap<String, String>>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let old_date_str = form
        .get("old_date")
        .ok_or_else(|| AppError::BadRequest("Missing old_date".into()))?;
    let new_date_str = form
        .get("new_date")
        .ok_or_else(|| AppError::BadRequest("Missing new_date".into()))?;
    let old_date = NaiveDate::parse_from_str(old_date_str, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid old date format. Use YYYY-MM-DD.".into()))?;

    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;

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

    match portfolio::rename_date(&state.db().await, portfolio_id, old_date, new_date).await {
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
    Query(query): Query<DateQuery>,
) -> Result<maud::Markup, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let date = NaiveDate::parse_from_str(&query.date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date format. Use YYYY-MM-DD.".into()))?;

    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;
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

// ── Portfolio CSV Import/Export ──

pub async fn portfolio_import(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
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
                    input type="file" name="csv_file" accept=".csv,.txt" required
                           id=(format!("csv-file-{}", portfolio_id))
                           onchange=(format!("document.getElementById('upload-btn-{}').disabled = !this.files.length", portfolio_id)) {}
                }
                button type="submit" class="btn" id=(format!("upload-btn-{}", portfolio_id)) disabled { "Upload & Preview" }
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
    mut multipart: Multipart,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;

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
    let analysis = csv_import::analyze_csv(&csv_data)?;

    // Save to temp file for confirm step
    let tmp_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string();
    let tmp_path = format!(
        "/tmp/financials_portfolio_csv_{}_{}.csv",
        portfolio_id, tmp_id
    );
    std::fs::write(&tmp_path, &csv_data)
        .map_err(|e| AppError::BadRequest(format!("Failed to save CSV: {}", e)))?;

    // Load existing wealth items for mapping
    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;

    let num_cols = analysis.preview_rows.first().map(|r| r.len()).unwrap_or(0);
    let col_numbers: Vec<usize> = (0..num_cols).collect();

    // Detect which column looks like a date
    let date_col = analysis.detected.date_col;

    // Grab a sample date from the first preview row to show format examples
    let date_examples: Vec<(&'static str, String)> = analysis
        .preview_rows
        .first()
        .and_then(|row| {
            let sample = row.get(date_col)?;
            if sample.is_empty() {
                return None;
            }
            crate::views::portfolio::date_format_examples(sample, &analysis.detected.date_format)
        })
        .unwrap_or_default();

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
                        @for (fmt, example) in &date_examples {
                            @let selected = if *fmt == analysis.detected.date_format { " selected" } else { "" };
                            option value=(fmt) selected[selected == " selected"] { (format!("{}  →  {}", fmt, example)) }
                        }
                    }

                    @for col_idx in 0..num_cols {
                        @if col_idx == date_col {
                            // Skip date column in item mapping — it's handled above
                        } @else {
                            div class="csv-mapping-row" style="margin: 0.5em 0; padding: 0.5em; border: 1px solid var(--border); border-radius: 4px;" {
                                @let col_header = analysis.headers.get(col_idx).map(|s| s.as_str()).unwrap_or("");
                                @let col_label = if col_header.is_empty() { format!("Column {}", col_idx + 1) } else { format!("Column {}: {}", col_idx + 1, col_header) };
                                // Show first-row example data
                                @let example_val = analysis.preview_rows.first().and_then(|row| row.get(col_idx)).map(|s| s.as_str()).unwrap_or("");
                                // Auto-match: if the column header matches an existing item name, select it by default
                                @let matched_item_id = items.iter().find(|item| item.name.eq_ignore_ascii_case(col_header)).map(|item| item.item_id);
                                strong { (col_label) }
                                @if !example_val.is_empty() {
                                    span class="csv-example" style="margin-left: 0.5em; font-size: 0.85em; color: var(--text-muted);" { (format!("e.g. {}", example_val)) }
                                }
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
) -> Result<Redirect, AppError> {
    portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;

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
                    let col_str = &key[4..key.len() - 5];
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

    let result = portfolio::import_csv(&state.db().await, portfolio_id, &raw, &mapping).await?;

    let flash_msg = format!(
        "Imported {} rows ({} skipped, {} items created)",
        result.rows_imported, result.rows_skipped, result.items_created
    );
    let encoded = flash_msg
        .replace(' ', "+")
        .replace('%', "%25")
        .replace('&', "%26");

    Ok(Redirect::to(&format!(
        "/portfolio/{}?flash={}&flash_type=success",
        portfolio_id, encoded
    )))
}

pub async fn portfolio_csv(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let (_id, name) = portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;

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

// ── URL decode helper ──

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
