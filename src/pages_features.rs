use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::flash::Flash;
use crate::layout::{active, active_flash};
use crate::models::features::*;
use crate::models::portfolio;
use crate::utils;
use axum::extract::{Form, Path, Query, State};
use axum::response::IntoResponse;
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::BTreeMap;
use uuid::Uuid;

// ── Helpers ──

fn format_cents(cents: i64) -> String {
    utils::format_cents(cents)
}

const CATEGORIES: &[&str] = &[
    "Housing", "Transportation", "Food & Dining", "Utilities",
    "Insurance", "Healthcare", "Entertainment", "Shopping",
    "Gifts", "Subscriptions", "Personal Care", "Savings",
    "Investment", "Income", "Other",
];

// ── Transactions ──

#[derive(Deserialize)]
pub struct TxnFilter {
    pub from: Option<String>,
    pub to: Option<String>,
    pub category: Option<String>,
    pub txn_type: Option<String>,
    pub page: Option<i64>,
    pub search: Option<String>,
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

const PER_PAGE: i64 = 20;

pub async fn transactions(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<TxnFilter>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.db;
    let from = filter.from.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let to = filter.to.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let cat = filter.category.as_deref();
    let tt = filter.txn_type.as_deref();
    let search = filter.search.as_deref();
    let page = filter.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;

    let total_count = count_transactions(pool, &user.0, from, to, cat, tt, search).await?;
    let total_pages = ((total_count as f64) / (PER_PAGE as f64)).ceil() as i64;

    let txns = list_transactions(pool, &user.0, from, to, cat, tt, search, Some(PER_PAGE), Some(offset)).await?;

    // Use aggregate query for summary totals (across all matching results, not just current page)
    let (total_income, total_expenses) = sum_transactions(pool, &user.0, from, to, cat, tt, search).await?;
    let net = total_income - total_expenses;

    let from_val = filter.from.as_deref().unwrap_or("");
    let to_val = filter.to.as_deref().unwrap_or("");
    let cat_val = filter.category.as_deref().unwrap_or("");
    let type_val = filter.txn_type.as_deref().unwrap_or("");
    let search_val = filter.search.as_deref().unwrap_or("");

    // Build query string for pagination links (preserving filters)
    let mut qs_parts = Vec::new();
    if !from_val.is_empty() { qs_parts.push(format!("from={}", from_val)); }
    if !to_val.is_empty() { qs_parts.push(format!("to={}", to_val)); }
    if !cat_val.is_empty() { qs_parts.push(format!("category={}", cat_val)); }
    if !type_val.is_empty() { qs_parts.push(format!("txn_type={}", type_val)); }
    if !search_val.is_empty() { qs_parts.push(format!("search={}", search_val.replace(' ', "+"))); }
    let qs_base = if qs_parts.is_empty() { String::new() } else { format!("&{}", qs_parts.join("&")) };
    let csv_qs = {
        let mut p = Vec::new();
        if !from_val.is_empty() { p.push(format!("from={}", from_val)); }
        if !to_val.is_empty() { p.push(format!("to={}", to_val)); }
        if !cat_val.is_empty() { p.push(format!("category={}", cat_val)); }
        if !type_val.is_empty() { p.push(format!("txn_type={}", type_val)); }
        if !search_val.is_empty() { p.push(format!("search={}", search_val.replace(' ', "+"))); }
        if p.is_empty() { "/transactions/export/csv".to_string() } else { format!("/transactions/export/csv?{}", p.join("&")) }
    };

    let cat_options: String = CATEGORIES.iter().map(|c| {
        let sel = if *c == cat_val { " selected" } else { "" };
        format!("<option value=\"{}\"{}>{}</option>", c, sel, c)
    }).collect();

    let type_options = "<option value=\"\">All</option><option value=\"income\">Income</option><option value=\"expense\">Expense</option>";

    let txn_rows: String = txns.iter().map(|t| {
        let cls = if t.amount >= 0 { "positive" } else { "negative" };
        let sign = if t.amount >= 0 { "+" } else { "-" };
        let type_chip = if t.txn_type == "income" { "chip-income" } else { "chip-expense" };
        format!(
            "<tr><td>{}</td><td><span class=\"chip {}\">{}</span></td><td>{}</td><td class=\"{}\">{}{}</td>\
             <td><a href=\"/transactions/{}/edit\" class=\"btn btn-xs\">Edit</a> \
             <form method=\"post\" action=\"/transactions/{}\" style=\"display:inline\" onsubmit=\"return confirm('Delete this transaction?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-xs\">×</button></form></td></tr>",
            t.txn_date, type_chip, utils::html_escape(&t.txn_type), utils::html_escape(&t.description), cls, sign, format_cents(t.amount.abs()),
            t.txn_id, t.txn_id
        )
    }).collect();

    let net_cls = if net >= 0 { "summary-value positive" } else { "summary-value negative" };

    // Pagination controls
    let pagination_html = if total_pages > 1 {
        let prev_link = if page > 1 { format!("/transactions?page={}{}", page - 1, qs_base) } else { String::new() };
        let next_link = if page < total_pages { format!("/transactions?page={}{}", page + 1, qs_base) } else { String::new() };
        format!(
            "<div class=\"pagination\"><span class=\"page-info\">Page {} of {} ({} transactions)</span>\
             {}{}\
             <a href=\"/transactions?page={}{}\" class=\"btn btn-sm\">First</a>\
             <a href=\"/transactions?page={}{}\" class=\"btn btn-sm\">Last</a></div>",
            page, total_pages, total_count,
            if page > 1 { format!("<a href=\"{}\" class=\"btn btn-sm\">← Prev</a>", prev_link) } else { "<span class=\"btn btn-sm btn-disabled\">← Prev</span>".to_string() },
            if page < total_pages { format!("<a href=\"{}\" class=\"btn btn-sm\">Next →</a>", next_link) } else { "<span class=\"btn btn-sm btn-disabled\">Next →</span>".to_string() },
            1, qs_base, total_pages, qs_base
        )
    } else {
        String::new()
    };

    let html = active_flash("Transactions", maud::html! {
        div class="page-header" {
            h2 { "Transactions" }
        }
        div class="filter-bar" {
            div class="quick-select" {
                span class="quick-select-label" { "Quick:" }
                button type="button" class="btn btn-xs" onclick="setRange('month')" { "This Month" }
                button type="button" class="btn btn-xs" onclick="setRange('last')" { "Last Month" }
                button type="button" class="btn btn-xs" onclick="setRange('30d')" { "Last 30 Days" }
                button type="button" class="btn btn-xs" onclick="setRange('year')" { "This Year" }
            }
            form method="get" action="/transactions" class="filter-form" {
                label { "From" input type="date" name="from" id="filter-from" value=(from_val) {} }
                label { "To" input type="date" name="to" id="filter-to" value=(to_val) {} }
                label { "Category" select name="category" {
                    option value="" { "All Categories" }
                    (maud::PreEscaped(cat_options))
                }}
                label { "Type" select name="txn_type" {
                    (maud::PreEscaped(type_options))
                }}
                label { "Search" input type="text" name="search" value=(search_val) placeholder="Description..." {} }
                button type="submit" class="btn btn-primary btn-sm" { "Apply" }
                a href="/transactions" class="btn btn-sm" { "Reset" }
            }
            script { (maud::PreEscaped(r#"function setRange(p){const f=document.getElementById('filter-from'),t=document.getElementById('filter-to'),d=new Date(),y=d.getFullYear(),m=d.getMonth();let a,b;if(p==='month'){a=new Date(y,m,1);b=new Date(y,m+1,0)}else if(p==='last'){a=new Date(y,m-1,1);b=new Date(y,m,0)}else if(p==='30d'){a=new Date(d);a.setDate(a.getDate()-30);b=d}else{a=new Date(y,0,1);b=d}f.value=a.toISOString().slice(0,10);t.value=b.toISOString().slice(0,10);t.closest('form').submit()}"#)) }
        }
        div class="summary-cards" {
            div class="summary-card" {
                span class="summary-label" { "Transactions" }
                span class="summary-value" { (total_count.to_string()) }
            }
            div class="summary-card" {
                span class="summary-label" { "Income" }
                span class="summary-value positive" { (format_cents(total_income)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Expenses" }
                span class="summary-value negative" { (format_cents(total_expenses)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Net" }
                span class=(net_cls) { (format_cents(net)) }
            }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Transaction List" }
                div {
                    a href=(csv_qs) class="btn btn-sm" { "Export CSV" }
                    a href="/transactions/new" class="btn btn-primary btn-sm" { "+ Add Transaction" }
                }
            }
            div class="table-responsive" {
                table class="data-table" {
                    thead { tr { th { "Date" } th { "Type" } th { "Description" } th { "Amount" } th {} } }
                    tbody { (maud::PreEscaped(txn_rows)) }
                }
            }
            (maud::PreEscaped(pagination_html))
        }
    }, Some(&user), false, "transactions", &Flash { flash: filter.flash.clone(), flash_type: filter.flash_type.clone() }).into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct NewTxnForm {
    pub txn_date: String,
    pub amount: String,
    pub category: String,
    pub description: String,
    pub txn_type: String,
}

pub async fn transactions_new(
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let cat_options: String = CATEGORIES.iter().map(|c| {
        format!("<option value=\"{}\">{}</option>", c, c)
    }).collect();

    let html = active("New Transaction", maud::html! {
        div class="page-header" { h2 { "New Transaction" } }
        div class="content-card" {
            form method="post" action="/transactions/new" class="form-grid" {
                label { "Date" input type="date" name="txn_date" required {} }
                label { "Type" select name="txn_type" {
                    option value="expense" { "Expense" }
                    option value="income" { "Income" }
                }}
                label { "Category" select name="category" {
                    (maud::PreEscaped(cat_options))
                }}
                label { "Description" input type="text" name="description" placeholder="What was this for?" {} }
                label { "Amount ($)" input type="number" name="amount" step="0.01" min="0" required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Add Transaction" }
                    a href="/transactions" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "transactions").into_response();
    Ok(html)
}

pub async fn transactions_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<NewTxnForm>,
) -> Result<impl IntoResponse, AppError> {
    let date = utils::validate_date(&form.txn_date, "date")?;
    let amount_cents = utils::validate_amount(&form.amount, "Amount")?;
    let amount = if form.txn_type == "expense" { -amount_cents } else { amount_cents };

    create_transaction(&state.db, &user.0, date, amount, &form.category, &form.description, &form.txn_type).await?;

    Ok(axum::response::Redirect::to("/transactions?flash=Transaction+created&flash_type=success").into_response())
}

pub async fn transactions_delete(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let txn_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_transaction(&state.db, &user.0, txn_id).await?;
    Ok(axum::response::Redirect::to("/transactions?flash=Transaction+deleted&flash_type=success").into_response())
}

pub async fn transactions_edit(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let txn_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let txn = get_transaction(&state.db, &user.0, txn_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Transaction not found".into()))?;

    let amount_display = if txn.amount >= 0 {
        format_cents(txn.amount)
    } else {
        format!("-{}", format_cents(txn.amount.abs()))
    };
    let date_val = txn.txn_date.to_string();
    let cat_options: String = CATEGORIES.iter().map(|c| {
        let sel = if *c == txn.category { " selected" } else { "" };
        format!("<option value=\"{}\"{}>{}</option>", c, sel, c)
    }).collect();

    let edit_action = format!("/transactions/{}/edit", id);
    let html = active("Edit Transaction", maud::html! {
        div class="page-header" {
            h2 { "Edit Transaction" }
        }
        div class="content-card" {
            form method="post" action=(edit_action) class="form-grid" {
                label { "Date" input type="date" name="txn_date" value=(date_val) required {} }
                label { "Type"
                    select name="txn_type" {
                        (maud::PreEscaped(format!("<option value=\"expense\"{}>Expense</option><option value=\"income\"{}>Income</option>",
                            if txn.txn_type == "expense" { " selected" } else { "" },
                            if txn.txn_type == "income" { " selected" } else { "" }
                        )))
                    }
                }
                label { "Category"
                    select name="category" {
                        (maud::PreEscaped(cat_options))
                    }
                }
                label { "Description" input type="text" name="description" value=(txn.description) {} }
                label { "Amount ($)" input type="text" name="amount" value=(amount_display) required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Save Changes" }
                    a href="/transactions" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "transactions").into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct EditTransactionForm {
    pub txn_date: String,
    pub txn_type: String,
    pub category: String,
    pub description: String,
    pub amount: String,
}

pub async fn transactions_update(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<EditTransactionForm>,
) -> Result<impl IntoResponse, AppError> {
    let txn_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let date = utils::validate_date(&form.txn_date, "Date")?;
    let amount = utils::validate_amount(&form.amount, "Amount")?;
    let txn_type = if form.txn_type == "income" { "income" } else { "expense" };
    let amount = if txn_type == "expense" { -amount.abs() } else { amount.abs() };

    update_transaction(&state.db, &user.0, txn_id, date, amount, &form.category, &form.description, txn_type).await?;
    Ok(axum::response::Redirect::to("/transactions?flash=Transaction+updated&flash_type=success").into_response())
}

pub async fn transactions_csv(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<TxnFilter>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.db;
    let from = filter.from.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let to = filter.to.as_deref().and_then(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());
    let cat = filter.category.as_deref();
    let tt = filter.txn_type.as_deref();
    let search = filter.search.as_deref();

    let txns = list_transactions(pool, &user.0, from, to, cat, tt, search, None, None).await?;

    let mut wtr = csv::Writer::from_writer(Vec::new());
    wtr.write_record(&["Date", "Type", "Category", "Description", "Amount"])
        .map_err(|e| AppError::Internal(e.into()))?;
    for t in &txns {
        let abs = t.amount.abs();
        let dollars = abs / 100;
        let cents_part = abs % 100;
        let amount_dollars = format!("{}{}.{:02}", if t.amount < 0 { "-" } else { "" }, dollars, cents_part);
        wtr.write_record(&[t.txn_date.to_string(), t.txn_type.clone(), t.category.clone(), t.description.clone(), amount_dollars])
            .map_err(|e| AppError::Internal(e.into()))?;
    }
    let data = wtr.into_inner().map_err(|e| AppError::Internal(e.into()))?;

    Ok((
        [
            ("content-type", "text/csv"),
            ("content-disposition", "attachment; filename=\"transactions.csv\""),
        ],
        data,
    ).into_response())
}

// ── Budgeting ──

#[derive(Deserialize)]
pub struct BudgetFilter {
    pub month: Option<String>,
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

pub async fn budgets(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<BudgetFilter>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.db;
    let now = chrono::Local::now();
    let month = filter.month.unwrap_or_else(|| now.format("%Y-%m").to_string());

    let budgets = list_budgets_for_month(pool, &user.0, &month).await?;
    let spending = monthly_spending_by_category(pool, &user.0, &month).await?;

    let spending_map: BTreeMap<String, i64> = spending.iter().map(|(c, a)| (c.clone(), *a)).collect();

    let total_budgeted: i64 = budgets.iter().map(|b| b.planned_amount).sum();
    let total_spent: i64 = spending.iter().map(|(_, a)| *a).sum();
    let remaining = total_budgeted - total_spent;

    let budget_rows: String = budgets.iter().map(|b| {
        let spent = spending_map.get(&b.category).copied().unwrap_or(0);
        let pct = if b.planned_amount > 0 { (spent as f64 / b.planned_amount as f64 * 100.0).min(100.0) } else { 0.0 };
        let bar_class = if pct > 90.0 { "bar-danger" } else if pct > 70.0 { "bar-warning" } else { "bar-success" };
        let over = spent > b.planned_amount;
        let diff = b.planned_amount as i64 - spent as i64;
        let diff_str = if over { format!("-{}", format_cents(diff.abs())) } else { format_cents(diff) };
        let diff_cls = if over { "negative" } else { "positive" };

        format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td class=\"{}\">{}</td><td class=\"{}\">{:.0}%</td>\
             <td><div class=\"progress-bar\"><div class=\"progress-fill {}\" style=\"width:{:.0}%\"></div></div></td>\
             <td><a href=\"/budgets/{}/edit\" class=\"btn btn-xs\">Edit</a> \
             <form method=\"post\" action=\"/budgets/{}/delete\" style=\"display:inline\" onsubmit=\"return confirm('Delete this budget entry?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-xs\">×</button></form></td></tr>",
            utils::html_escape(&b.category), format_cents(b.planned_amount), format_cents(spent),
            diff_cls, diff_str,
            if over { "negative" } else { "positive" }, pct,
            bar_class, pct,
            b.budget_id, b.budget_id
        )
    }).collect();

    let unbudgeted: String = spending.iter()
        .filter(|(c, _)| !budgets.iter().any(|b| b.category == *c))
        .map(|(c, a)| format!("<tr><td>{}</td><td>—</td><td>{}</td><td class=\"negative\">{}</td><td class=\"negative\">100%</td><td><div class=\"progress-bar\"><div class=\"progress-fill bar-danger\" style=\"width:100%\"></div></div></td></tr>", utils::html_escape(c), format_cents(*a), format_cents(-(*a as i64))))
        .collect();

    let rem_cls = if remaining >= 0 { "summary-value positive" } else { "summary-value negative" };

    let html = active_flash("Budgeting", maud::html! {
        div class="page-header" {
            h2 { "Budgeting" }
        }
        div class="filter-bar" {
            form method="get" action="/budgets" class="filter-form" {
                label { "Month" input type="month" name="month" value=(&month) {} }
                button type="submit" class="btn btn-primary btn-sm" { "Go" }
            }
        }
        div class="summary-cards" {
            div class="summary-card" {
                span class="summary-label" { "Total Budgeted" }
                span class="summary-value" { (format_cents(total_budgeted)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Total Spent" }
                span class="summary-value negative" { (format_cents(total_spent)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Remaining" }
                span class=(rem_cls) { (format_cents(remaining)) }
            }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Budget vs Actual — " (month) }
                a href="/budgets/new" class="btn btn-primary btn-sm" { "+ Add Budget" }
            }
            div class="table-responsive" {
                table class="data-table" {
                    thead { tr { th { "Category" } th { "Budgeted" } th { "Spent" } th { "Remaining" } th { "%" } th { "Progress" } th { "Actions" } } }
                    tbody {
                        (maud::PreEscaped(budget_rows))
                        (maud::PreEscaped(unbudgeted))
                    }
                }
            }
        }
    }, Some(&user), false, "budgets", &Flash { flash: filter.flash.clone(), flash_type: filter.flash_type.clone() }).into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct NewBudgetForm {
    pub category: String,
    pub month: String,
    pub planned_amount: String,
}

pub async fn budgets_new(
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let cat_options: String = CATEGORIES.iter().map(|c| {
        format!("<option value=\"{}\">{}</option>", c, c)
    }).collect();
    let now = chrono::Local::now().format("%Y-%m").to_string();

    let html = active("New Budget", maud::html! {
        div class="page-header" { h2 { "New Budget Entry" } }
        div class="content-card" {
            form method="post" action="/budgets/new" class="form-grid" {
                label { "Category" select name="category" { (maud::PreEscaped(cat_options)) } }
                label { "Month" input type="month" name="month" value=(&now) {} }
                label { "Planned Amount ($)" input type="number" name="planned_amount" step="0.01" min="0" required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Save Budget" }
                    a href="/budgets" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "budgets").into_response();
    Ok(html)
}

pub async fn budgets_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<NewBudgetForm>,
) -> Result<impl IntoResponse, AppError> {
    utils::validate_month(&form.month)?;
    let amount = utils::validate_amount(&form.planned_amount, "Planned amount")?;
    create_or_update_budget(&state.db, &user.0, &form.category, &form.month, amount).await?;
    Ok(axum::response::Redirect::to(&format!("/budgets?month={}&flash=Budget+saved&flash_type=success", form.month)).into_response())
}

pub async fn budgets_edit(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let budget_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let budget = get_budget(&state.db, &user.0, budget_id).await?
        .ok_or_else(|| AppError::BadRequest("Budget not found".into()))?;
    let cat_options: String = CATEGORIES.iter().map(|c| {
        let sel = if *c == budget.category { " selected" } else { "" };
        format!("<option value=\"{}\"{}>{}</option>", c, sel, c)
    }).collect();

    let edit_action = format!("/budgets/{}/edit", id);
    let html = active("Edit Budget", maud::html! {
        div class="page-header" { h2 { "Edit Budget" } }
        div class="content-card" {
            form method="post" action=(edit_action) class="form-grid" {
                label { "Category" select name="category" { (maud::PreEscaped(cat_options)) } }
                label { "Month" input type="month" name="month" value=(&budget.month) {} }
                label { "Planned Amount ($)" input type="number" name="planned_amount" step="0.01" min="0" value=(format_cents(budget.planned_amount)) required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Update Budget" }
                    a href="/budgets" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "budgets").into_response();
    Ok(html)
}

pub async fn budgets_update(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<NewBudgetForm>,
) -> Result<impl IntoResponse, AppError> {
    let budget_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    // Verify ownership
    let _budget = get_budget(&state.db, &user.0, budget_id).await?
        .ok_or_else(|| AppError::BadRequest("Budget not found".into()))?;
    utils::validate_month(&form.month)?;
    let amount = utils::validate_amount(&form.planned_amount, "Planned amount")?;
    // Delete old and create new (handles category/month change)
    delete_budget(&state.db, &user.0, budget_id).await?;
    create_or_update_budget(&state.db, &user.0, &form.category, &form.month, amount).await?;
    Ok(axum::response::Redirect::to(&format!("/budgets?month={}&flash=Budget+updated&flash_type=success", form.month)).into_response())
}

pub async fn budgets_delete(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let budget_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_budget(&state.db, &user.0, budget_id).await?;
    Ok(axum::response::Redirect::to("/budgets?flash=Budget+deleted&flash_type=success").into_response())
}

// ── Savings Goals / Big Purchases ──

#[derive(Deserialize)]
pub struct GoalsFilter {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

pub async fn goals(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<GoalsFilter>,
) -> Result<impl IntoResponse, AppError> {
    let goals = list_savings_goals(&state.db, &user.0).await?;

    let goal_cards: String = goals.iter().map(|g| {
        let pct = if g.target_amount > 0 { (g.current_amount as f64 / g.target_amount as f64 * 100.0).min(100.0) } else { 0.0 };
        let bar_class = if pct >= 100.0 { "bar-success" } else if pct >= 50.0 { "bar-warning" } else { "bar-danger" };
        let remaining = g.target_amount - g.current_amount;
        let target_date_str = g.target_date.as_deref().unwrap_or("No deadline");
        let chip_cls = if pct >= 100.0 { "chip-success" } else if pct >= 50.0 { "chip-warning" } else { "chip-danger" };

        format!(
            "<div class=\"goal-card\">\
             <div class=\"goal-header\"><h3>{}</h3><span class=\"chip {}\">{:.0}%</span></div>\
             <div class=\"goal-progress\"><div class=\"progress-bar\"><div class=\"progress-fill {}\" style=\"width:{:.0}%\"></div></div></div>\
             <div class=\"goal-stats\">\
             <div class=\"goal-stat\"><span class=\"goal-label\">Saved</span><span class=\"goal-value\">{}</span></div>\
             <div class=\"goal-stat\"><span class=\"goal-label\">Target</span><span class=\"goal-value\">{}</span></div>\
             <div class=\"goal-stat\"><span class=\"goal-label\">Remaining</span><span class=\"goal-value negative\">{}</span></div>\
             <div class=\"goal-stat\"><span class=\"goal-label\">Deadline</span><span class=\"goal-value\">{}</span></div>\
             </div>\
             <div class=\"goal-actions\">\
             <a href=\"/goals/{}/edit\" class=\"btn btn-sm\">Edit</a>\
             <form method=\"post\" action=\"/goals/{}/update\" style=\"display:inline\">\
             <input type=\"number\" name=\"amount\" step=\"0.01\" placeholder=\"Update saved\" class=\"input-sm\" style=\"width:140px\">\
             <button type=\"submit\" class=\"btn btn-sm\">Update</button></form>\
             <form method=\"post\" action=\"/goals/{}/delete\" style=\"display:inline\" onsubmit=\"return confirm('Delete this goal?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-sm\">Delete</button></form>\
             </div></div>",
            utils::html_escape(&g.name), chip_cls, pct,
            bar_class, pct,
            format_cents(g.current_amount), format_cents(g.target_amount), format_cents(remaining),
            utils::html_escape(target_date_str),
            g.goal_id, g.goal_id, g.goal_id
        )
    }).collect();

    let html = active_flash("Savings Goals", maud::html! {
        div class="page-header" {
            h2 { "Savings Goals & Big Purchases" }
        }
        div class="goals-grid" {
            (maud::PreEscaped(goal_cards))
            div class="goal-card goal-card-add" {
                a href="/goals/new" { h3 { "+ New Goal" } }
            }
        }
    }, Some(&user), false, "goals", &Flash { flash: filter.flash.clone(), flash_type: filter.flash_type.clone() }).into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct NewGoalForm {
    pub name: String,
    pub target_amount: String,
    pub current_amount: Option<String>,
    pub target_date: Option<String>,
    pub category: String,
}

pub async fn goals_new(
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let html = active("New Savings Goal", maud::html! {
        div class="page-header" { h2 { "New Savings Goal" } }
        div class="content-card" {
            form method="post" action="/goals/new" class="form-grid" {
                label { "Goal Name" input type="text" name="name" placeholder="New Car, Vacation Fund..." required {} }
                label { "Target Amount ($)" input type="number" name="target_amount" step="0.01" min="0" required {} }
                label { "Currently Saved ($)" input type="number" name="current_amount" step="0.01" min="0" {} }
                label { "Target Date" input type="date" name="target_date" {} }
                label { "Category" select name="category" {
                    option value="general" { "General" }
                    option value="vehicle" { "Vehicle" }
                    option value="home" { "Home" }
                    option value="travel" { "Travel" }
                    option value="education" { "Education" }
                    option value="tech" { "Technology" }
                    option value="other" { "Other" }
                }}
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Create Goal" }
                    a href="/goals" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "goals").into_response();
    Ok(html)
}

pub async fn goals_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<NewGoalForm>,
) -> Result<impl IntoResponse, AppError> {
    let target = utils::validate_amount(&form.target_amount, "Target amount")?;
    let current = form.current_amount.as_deref()
        .map(|s| utils::parse_dollars(s))
        .transpose()
        .map_err(|e| AppError::BadRequest(e))?
        .unwrap_or(0);
    let target_date = form.target_date.as_deref().filter(|s| !s.is_empty());

    create_savings_goal(&state.db, &user.0, &form.name, target, current, target_date, &form.category).await?;
    Ok(axum::response::Redirect::to("/goals?flash=Goal+created&flash_type=success").into_response())
}

#[derive(Deserialize)]
pub struct UpdateGoalAmountForm {
    pub amount: String,
}

pub async fn goals_update_amount(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateGoalAmountForm>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let amount = utils::validate_amount(&form.amount, "Amount")?;
    update_savings_goal_amount(&state.db, &user.0, goal_id, amount).await?;
    Ok(axum::response::Redirect::to("/goals?flash=Goal+updated&flash_type=success").into_response())
}

pub async fn goals_delete(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_savings_goal(&state.db, &user.0, goal_id).await?;
    Ok(axum::response::Redirect::to("/goals?flash=Goal+deleted&flash_type=success").into_response())
}

pub async fn goals_edit(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let goal = get_savings_goal(&state.db, &user.0, goal_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Goal not found".into()))?;

    let target_display = format_cents(goal.target_amount);
    let current_display = format_cents(goal.current_amount);
    let date_val = goal.target_date.as_deref().unwrap_or("");
    let cat_options: String = CATEGORIES.iter().map(|c| {
        let sel = if *c == goal.category { " selected" } else { "" };
        format!("<option value=\"{}\"{}>{}</option>", c, sel, c)
    }).collect();

    let edit_action = format!("/goals/{}/edit", id);
    let html = active("Edit Goal", maud::html! {
        div class="page-header" {
            h2 { "Edit Savings Goal" }
        }
        div class="content-card" {
            form method="post" action=(edit_action) class="form-grid" {
                label { "Name" input type="text" name="name" value=(goal.name) required {} }
                label { "Target Amount ($)" input type="text" name="target_amount" value=(target_display) required {} }
                label { "Current Amount ($)" input type="text" name="current_amount" value=(current_display) required {} }
                label { "Target Date" input type="date" name="target_date" value=(date_val) {} }
                label { "Category"
                    select name="category" {
                        (maud::PreEscaped(cat_options))
                    }
                }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Save Changes" }
                    a href="/goals" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "goals").into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct EditGoalForm {
    pub name: String,
    pub target_amount: String,
    pub current_amount: String,
    pub target_date: Option<String>,
    pub category: String,
}

pub async fn goals_update(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<EditGoalForm>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let target = utils::validate_amount(&form.target_amount, "Target amount")?;
    let current = utils::validate_amount(&form.current_amount, "Current amount")?;
    let target_date = form.target_date.as_deref().filter(|s| !s.is_empty());

    update_savings_goal(&state.db, &user.0, goal_id, &form.name, target, current, target_date, &form.category).await?;
    Ok(axum::response::Redirect::to("/goals?flash=Goal+updated&flash_type=success").into_response())
}

// ── Holidays ──

#[derive(Deserialize)]
pub struct HolidaysFilter {
    pub flash: Option<String>,
    pub flash_type: Option<String>,
}

pub async fn holidays(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<HolidaysFilter>,
) -> Result<impl IntoResponse, AppError> {
    let holidays = list_holidays(&state.db, &user.0).await?;

    let mut holiday_data = Vec::new();
    for h in &holidays {
        let (_, expenses) = sum_transactions(
            &state.db,
            &user.0,
            Some(h.start_date),
            Some(h.end_date),
            None,
            Some("expense"),
            None,
        ).await?;
        let count = count_transactions(
            &state.db,
            &user.0,
            Some(h.start_date),
            Some(h.end_date),
            None,
            Some("expense"),
            None,
        ).await?;
        holiday_data.push((h.clone(), expenses, count));
    }

    let holiday_rows: String = holiday_data.iter().map(|(h, total, count)| {
        format!(
            "<tr><td>{}</td><td>{} — {}</td><td>{}</td><td>{}</td>\
             <td><a href=\"/holidays/{}/edit\" class=\"btn btn-xs\">Edit</a> \
             <form method=\"post\" action=\"/holidays/{}/delete\" style=\"display:inline\" onsubmit=\"return confirm('Delete this holiday?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-xs\">×</button></form></td></tr>",
            utils::html_escape(&h.name), h.start_date, h.end_date, count, format_cents(*total), h.holiday_id, h.holiday_id
        )
    }).collect();

    let html = active_flash("Holidays", maud::html! {
        div class="page-header" {
            h2 { "Holidays & Special Periods" }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Holiday Periods" }
                a href="/holidays/new" class="btn btn-primary btn-sm" { "+ Add Holiday" }
            }
            div class="table-responsive" {
                table class="data-table" {
                    thead { tr { th { "Name" } th { "Date Range" } th { "Transactions" } th { "Total Spent" } th {} } }
                    tbody { (maud::PreEscaped(holiday_rows)) }
                }
            }
        }
    }, Some(&user), false, "holidays", &Flash { flash: filter.flash.clone(), flash_type: filter.flash_type.clone() }).into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct NewHolidayForm {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
}

pub async fn holidays_new(
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let html = active("New Holiday Period", maud::html! {
        div class="page-header" { h2 { "New Holiday Period" } }
        div class="content-card" {
            form method="post" action="/holidays/new" class="form-grid" {
                label { "Name" input type="text" name="name" placeholder="Christmas 2025, Summer Vacation..." required {} }
                label { "Start Date" input type="date" name="start_date" required {} }
                label { "End Date" input type="date" name="end_date" required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Create Holiday" }
                    a href="/holidays" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "holidays").into_response();
    Ok(html)
}

pub async fn holidays_create(
    State(state): State<AppState>,
    user: LoggedInUser,
    Form(form): Form<NewHolidayForm>,
) -> Result<impl IntoResponse, AppError> {
    let start = utils::validate_date(&form.start_date, "Start date")?;
    let end = utils::validate_date(&form.end_date, "End date")?;
    if end < start {
        return Err(AppError::BadRequest("End date must be after start date".into()));
    }
    create_holiday(&state.db, &user.0, &form.name, start, end).await?;
    Ok(axum::response::Redirect::to("/holidays?flash=Holiday+created&flash_type=success").into_response())
}

pub async fn holidays_delete(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let holiday_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_holiday(&state.db, &user.0, holiday_id).await?;
    Ok(axum::response::Redirect::to("/holidays?flash=Holiday+deleted&flash_type=success").into_response())
}

pub async fn holidays_edit(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let holiday_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let h = get_holiday(&state.db, &user.0, holiday_id)
        .await?
        .ok_or_else(|| AppError::BadRequest("Holiday not found".into()))?;

    let start_val = h.start_date.to_string();
    let end_val = h.end_date.to_string();
    let edit_action = format!("/holidays/{}/edit", id);

    let html = active("Edit Holiday", maud::html! {
        div class="page-header" {
            h2 { "Edit Holiday Period" }
        }
        div class="content-card" {
            form method="post" action=(edit_action) class="form-grid" {
                label { "Name" input type="text" name="name" value=(h.name) required {} }
                label { "Start Date" input type="date" name="start_date" value=(start_val) required {} }
                label { "End Date" input type="date" name="end_date" value=(end_val) required {} }
                div class="form-actions" {
                    button type="submit" class="btn btn-primary" { "Save Changes" }
                    a href="/holidays" class="btn" { "Cancel" }
                }
            }
        }
    }, Some(&user), false, "holidays").into_response();
    Ok(html)
}

#[derive(Deserialize)]
pub struct EditHolidayForm {
    pub name: String,
    pub start_date: String,
    pub end_date: String,
}

pub async fn holidays_update(
    State(state): State<AppState>,
    user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<EditHolidayForm>,
) -> Result<impl IntoResponse, AppError> {
    let holiday_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let start = utils::validate_date(&form.start_date, "Start date")?;
    let end = utils::validate_date(&form.end_date, "End date")?;
    if end < start {
        return Err(AppError::BadRequest("End date must be after start date".into()));
    }
    update_holiday(&state.db, &user.0, holiday_id, &form.name, start, end).await?;
    Ok(axum::response::Redirect::to("/holidays?flash=Holiday+updated&flash_type=success").into_response())
}

// ── Reconciliation ──

#[derive(Deserialize)]
pub struct ReconFilter {
    pub month: Option<String>,
}

pub async fn reconciliation(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<ReconFilter>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.db;
    let now = chrono::Local::now();
    let month = filter.month.unwrap_or_else(|| now.format("%Y-%m").to_string());

    // Get all portfolios and their latest balances (batch queries to avoid N+1)
    let portfolios = portfolio::list_portfolios(pool, &user.0).await?;
    let all_items = portfolio::list_all_wealth_items_for_user(pool, &user.0).await?;
    let all_logs = portfolio::list_all_balance_logs_for_user(pool, &user.0).await?;

    // Group items by portfolio_id
    let mut items_by_portfolio: std::collections::HashMap<uuid::Uuid, Vec<&portfolio::WealthItemWithPortfolio>> = std::collections::HashMap::new();
    for item in &all_items {
        items_by_portfolio.entry(item.portfolio_id).or_default().push(item);
    }

    // Build latest and prev values per item from all logs
    let mut latest_per_item: std::collections::HashMap<uuid::Uuid, (String, String, i64)> = std::collections::HashMap::new();
    let mut prev_per_item: std::collections::HashMap<uuid::Uuid, (String, i64)> = std::collections::HashMap::new();
    let y: i32 = month[..4].parse().unwrap();
    let m: u32 = month[5..7].parse().unwrap();
    let prev_month = if m == 1 { format!("{}-12", y - 1) } else { format!("{}-{:02}", y, m - 1) };

    for log in &all_logs {
        let item = all_items.iter().find(|i| i.item_id == log.item_id);
        let item_type = item.map(|i| i.item_type.as_str()).unwrap_or("asset");
        let key = log.item_id;
        let date_str = log.log_date.to_string();

        // Latest value
        latest_per_item.entry(key).and_modify(|e| {
            if date_str > e.0 { *e = (date_str.clone(), item_type.to_string(), log.balance_value); }
        }).or_insert((date_str.clone(), item_type.to_string(), log.balance_value));

        // Previous month value
        if date_str.starts_with(&prev_month) {
            prev_per_item.entry(key).and_modify(|e| {
                if date_str > e.0 { *e = (date_str.clone(), log.balance_value); }
            }).or_insert((date_str.clone(), log.balance_value));
        }
    }

    // Build portfolio_data by grouping
    let mut portfolio_data = Vec::new();
    for (pid, pname) in &portfolios {
        let items = items_by_portfolio.get(pid).map(|v| v.len()).unwrap_or(0);
        let items_in_portfolio = items_by_portfolio.get(pid).map(|v| v.as_slice()).unwrap_or(&[]);
        let total_latest: i64 = items_in_portfolio.iter().map(|item| {
            match latest_per_item.get(&item.item_id) {
                Some((itype, _, val)) => if itype == "debt" { -*val } else { *val },
                None => 0,
            }
        }).sum();
        let total_prev: i64 = items_in_portfolio.iter().map(|item| {
            prev_per_item.get(&item.item_id).map(|(_, v)| *v).unwrap_or_else(|| {
                latest_per_item.get(&item.item_id).map(|(_, _, v)| *v).unwrap_or(0)
            })
        }).sum();
        let total_prev = if prev_per_item.is_empty() { total_latest } else { total_prev };
        portfolio_data.push((pname.clone(), items, total_latest, total_prev));
    }

    // Get transaction totals and category breakdown for the month via SQL aggregates
    let month_start = format!("{}-01", month);
    let month_end = {
        let y: i32 = month[..4].parse().unwrap();
        let m: u32 = month[5..7].parse().unwrap();
        let next = NaiveDate::from_ymd_opt(y, m, 1).unwrap() + chrono::Duration::days(32);
        let last = NaiveDate::from_ymd_opt(next.year(), next.month(), 1).unwrap().pred_opt().unwrap();
        last.to_string()
    };
    let month_start_date = NaiveDate::parse_from_str(&month_start, "%Y-%m-%d").unwrap();
    let month_end_date = NaiveDate::parse_from_str(&month_end, "%Y-%m-%d").unwrap();

    let (total_income, total_expenses) = sum_transactions(
        pool, &user.0, Some(month_start_date), Some(month_end_date), None, None, None,
    ).await?;
    let net_flow = total_income - total_expenses;

    // Category breakdown via SQL aggregate
    let cat_totals = sum_transactions_by_category(
        pool, &user.0, Some(month_start_date), Some(month_end_date), None, None, None,
    ).await?;

    let cat_rows: String = cat_totals.iter().map(|(cat, inc, exp)| {
        format!("<tr><td>{}</td><td class=\"positive\">{}</td><td class=\"negative\">{}</td><td>{}</td></tr>",
            utils::html_escape(cat), format_cents(*inc), format_cents(*exp), format_cents(*inc as i64 - *exp as i64))
    }).collect();

    let portfolio_rows: String = portfolio_data.iter().map(|(name, items, latest, prev)| {
        let change = *latest as i64 - *prev as i64;
        let cls = if change >= 0 { "positive" } else { "negative" };
        format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"{}\">{}</td></tr>",
            utils::html_escape(name), items, format_cents(*prev), format_cents(*latest), cls, format_cents(change))
    }).collect();

    let net_cls = if net_flow >= 0 { "summary-value positive" } else { "summary-value negative" };

    let html = active("Reconciliation", maud::html! {
        div class="page-header" {
            h2 { "Reconciliation" }
        }
        div class="filter-bar" {
            form method="get" action="/reconciliation" class="filter-form" {
                label { "Month" input type="month" name="month" value=(&month) {} }
                button type="submit" class="btn btn-primary btn-sm" { "Go" }
            }
        }
        div class="summary-cards" {
            div class="summary-card" {
                span class="summary-label" { "Income" }
                span class="summary-value positive" { (format_cents(total_income)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Expenses" }
                span class="summary-value negative" { (format_cents(total_expenses)) }
            }
            div class="summary-card" {
                span class="summary-label" { "Net Flow" }
                span class=(net_cls) { (format_cents(net_flow)) }
            }
        }
            div class="content-card" {
            div class="content-card-header" {
                h3 { "Portfolio Balances — " (&month) }
            }
            div class="table-responsive" {
                table class="data-table" {
                    thead { tr { th { "Portfolio" } th { "Items" } th { "Previous" } th { "Current" } th { "Change" } } }
                    tbody { (maud::PreEscaped(portfolio_rows)) }
                }
            }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Spending by Category — " (&month) }
            }
            div class="table-responsive" {
                table class="data-table" {
                    thead { tr { th { "Category" } th { "Income" } th { "Expenses" } th { "Net" } } }
                    tbody { (maud::PreEscaped(cat_rows)) }
                }
            }
        }
    }, Some(&user), false, "reconciliation").into_response();
    Ok(html)
}