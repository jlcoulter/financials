use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::portfolio::{self, BalanceLog, WealthItem};
use crate::requests::PortfolioQuery;
use crate::utils;
use axum::extract::{Path, State};
use chrono::NaiveDate;
use uuid::Uuid;

pub async fn portfolio(
    Path(portfolio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
    axum::extract::Query(query): axum::extract::Query<PortfolioQuery>,
) -> Result<maud::Markup, AppError> {
    let (_id, name) = portfolio::get_portfolio(&state.db().await, portfolio_id, user.0).await?;
    let items = portfolio::list_wealth_items(&state.db().await, portfolio_id).await?;
    let logs = portfolio::list_balance_logs(&state.db().await, portfolio_id).await?;
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
                                        (utils::format_date(row.date))
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

/// Given a sample date string and its detected format, try to parse it and
/// return examples showing what each format in the dropdown would render as.
/// Returns `None` if the sample can't be parsed with the detected format.
pub fn date_format_examples(
    sample: &str,
    detected_format: &str,
) -> Option<Vec<(&'static str, String)>> {
    let date = NaiveDate::parse_from_str(sample, detected_format).ok()?;
    let formats: &[&'static str] = &[
        "%d/%m/%Y",
        "%d/%m/%y",
        "%Y-%m-%d",
        "%m/%d/%Y",
        "%m/%d/%y",
        "%Y/%m/%d",
        "%b %d, %Y",
        "%d %b %Y",
        "%B %d, %Y",
        "%d %B %y",
        "%d %B %Y",
    ];
    Some(
        formats
            .iter()
            .map(|fmt| (*fmt, date.format(fmt).to_string()))
            .collect(),
    )
}

pub struct GridRow {
    date: NaiveDate,
    values: Vec<Option<i64>>,
}

pub fn pivot_logs(items: &[WealthItem], logs: &[BalanceLog]) -> Vec<GridRow> {
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

pub async fn portfolios(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = portfolio::list_portfolios(&state.db().await, user.0).await?;
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
}
