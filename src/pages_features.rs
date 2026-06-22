use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::features::*;
use crate::models::portfolio;
use axum::extract::{Form, Path, Query, State};
use axum::response::IntoResponse;
use chrono::{Datelike, NaiveDate};
use serde::Deserialize;
use std::collections::BTreeMap;
use uuid::Uuid;

// ── Helpers ──

fn format_cents(cents: i64) -> String {
    let sign = if cents < 0 { "-" } else { "" };
    let abs = cents.abs();
    let dollars = abs / 100;
    let remainder = abs % 100;
    format!("{}${}.{:02}", sign, dollars, remainder)
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
}

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

    let txns = list_transactions(pool, from, to, cat, tt).await?;

    let total_income: i64 = txns.iter().filter(|t| t.amount > 0).map(|t| t.amount).sum();
    let total_expenses: i64 = txns.iter().filter(|t| t.amount < 0).map(|t| t.amount.abs()).sum();
    let net = total_income - total_expenses;

    let from_val = filter.from.as_deref().unwrap_or("");
    let to_val = filter.to.as_deref().unwrap_or("");
    let cat_val = filter.category.as_deref().unwrap_or("");
    let _type_val = filter.txn_type.as_deref().unwrap_or("");

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
             <td><form method=\"post\" action=\"/transactions/{}\" style=\"display:inline\" onsubmit=\"return confirm('Delete this transaction?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-xs\">×</button></form></td></tr>",
            t.txn_date, type_chip, t.txn_type, t.description, cls, sign, format_cents(t.amount.abs()),
            t.txn_id
        )
    }).collect();

    let net_cls = if net >= 0 { "summary-value positive" } else { "summary-value negative" };

    let html = layout("Transactions", maud::html! {
        div class="page-header" {
            h2 { "Transactions" }
        }
        div class="filter-bar" {
            form method="get" action="/transactions" class="filter-form" {
                label { "From" input type="date" name="from" value=(from_val) {} }
                label { "To" input type="date" name="to" value=(to_val) {} }
                label { "Category" select name="category" {
                    option value="" { "All Categories" }
                    (maud::PreEscaped(cat_options))
                }}
                label { "Type" select name="txn_type" {
                    (maud::PreEscaped(type_options))
                }}
                button type="submit" class="btn btn-primary btn-sm" { "Apply" }
                a href="/transactions" class="btn btn-sm" { "Reset" }
            }
        }
        div class="summary-cards" {
            div class="summary-card" {
                span class="summary-label" { "Transactions" }
                span class="summary-value" { (txns.len().to_string()) }
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
                a href="/transactions/new" class="btn btn-primary btn-sm" { "+ Add Transaction" }
            }
            table class="data-table" {
                thead { tr { th { "Date" } th { "Type" } th { "Description" } th { "Amount" } th {} } }
                tbody { (maud::PreEscaped(txn_rows)) }
            }
        }
    }, Some(&user), false).into_response();
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

    let html = layout("New Transaction", maud::html! {
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
    }, Some(&user), false).into_response();
    Ok(html)
}

pub async fn transactions_create(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<NewTxnForm>,
) -> Result<impl IntoResponse, AppError> {
    let date = NaiveDate::parse_from_str(&form.txn_date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid date".into()))?;
    let amount_cents = (form.amount.parse::<f64>().map_err(|_| AppError::BadRequest("Invalid amount".into()))? * 100.0).round() as i64;
    let amount = if form.txn_type == "expense" { -amount_cents } else { amount_cents };

    create_transaction(&state.db, date, amount, &form.category, &form.description, &form.txn_type).await?;

    Ok(axum::response::Redirect::to("/transactions").into_response())
}

pub async fn transactions_delete(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let txn_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_transaction(&state.db, txn_id).await?;
    Ok(axum::response::Redirect::to("/transactions").into_response())
}

// ── Budgeting ──

#[derive(Deserialize)]
pub struct BudgetFilter {
    pub month: Option<String>,
}

pub async fn budgets(
    State(state): State<AppState>,
    user: LoggedInUser,
    Query(filter): Query<BudgetFilter>,
) -> Result<impl IntoResponse, AppError> {
    let pool = &state.db;
    let now = chrono::Local::now();
    let month = filter.month.unwrap_or_else(|| now.format("%Y-%m").to_string());

    let budgets = list_budgets_for_month(pool, &month).await?;
    let spending = monthly_spending_by_category(pool, &month).await?;

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
             <td><div class=\"progress-bar\"><div class=\"progress-fill {}\" style=\"width:{:.0}%\"></div></div></td></tr>",
            b.category, format_cents(b.planned_amount), format_cents(spent),
            diff_cls, diff_str,
            if over { "negative" } else { "positive" }, pct,
            bar_class, pct
        )
    }).collect();

    let unbudgeted: String = spending.iter()
        .filter(|(c, _)| !budgets.iter().any(|b| b.category == *c))
        .map(|(c, a)| format!("<tr><td>{}</td><td>—</td><td>{}</td><td class=\"negative\">{}</td><td class=\"negative\">100%</td><td><div class=\"progress-bar\"><div class=\"progress-fill bar-danger\" style=\"width:100%\"></div></div></td></tr>", c, format_cents(*a), format_cents(-(*a as i64))))
        .collect();

    let rem_cls = if remaining >= 0 { "summary-value positive" } else { "summary-value negative" };

    let html = layout("Budgeting", maud::html! {
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
            table class="data-table" {
                thead { tr { th { "Category" } th { "Budgeted" } th { "Spent" } th { "Remaining" } th { "%" } th { "Progress" } } }
                tbody {
                    (maud::PreEscaped(budget_rows))
                    (maud::PreEscaped(unbudgeted))
                }
            }
        }
    }, Some(&user), false).into_response();
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

    let html = layout("New Budget", maud::html! {
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
    }, Some(&user), false).into_response();
    Ok(html)
}

pub async fn budgets_create(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<NewBudgetForm>,
) -> Result<impl IntoResponse, AppError> {
    let amount = (form.planned_amount.parse::<f64>().map_err(|_| AppError::BadRequest("Invalid amount".into()))? * 100.0).round() as i64;
    create_or_update_budget(&state.db, &form.category, &form.month, amount).await?;
    Ok(axum::response::Redirect::to(&format!("/budgets?month={}", form.month)).into_response())
}

// ── Savings Goals / Big Purchases ──

pub async fn goals(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let goals = list_savings_goals(&state.db).await?;

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
             <form method=\"post\" action=\"/goals/{}/update\" style=\"display:inline\">\
             <input type=\"number\" name=\"amount\" step=\"0.01\" placeholder=\"Update saved\" class=\"input-sm\" style=\"width:140px\">\
             <button type=\"submit\" class=\"btn btn-sm\">Update</button></form>\
             <form method=\"post\" action=\"/goals/{}/delete\" style=\"display:inline\" onsubmit=\"return confirm('Delete this goal?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-sm\">Delete</button></form>\
             </div></div>",
            g.name, chip_cls, pct,
            bar_class, pct,
            format_cents(g.current_amount), format_cents(g.target_amount), format_cents(remaining),
            target_date_str,
            g.goal_id, g.goal_id
        )
    }).collect();

    let html = layout("Savings Goals", maud::html! {
        div class="page-header" {
            h2 { "Savings Goals & Big Purchases" }
        }
        div class="goals-grid" {
            (maud::PreEscaped(goal_cards))
            div class="goal-card goal-card-add" {
                a href="/goals/new" { h3 { "+ New Goal" } }
            }
        }
    }, Some(&user), false).into_response();
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
    let html = layout("New Savings Goal", maud::html! {
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
    }, Some(&user), false).into_response();
    Ok(html)
}

pub async fn goals_create(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<NewGoalForm>,
) -> Result<impl IntoResponse, AppError> {
    let target = (form.target_amount.parse::<f64>().map_err(|_| AppError::BadRequest("Invalid target amount".into()))? * 100.0).round() as i64;
    let current = form.current_amount.as_deref()
        .and_then(|s| s.parse::<f64>().ok())
        .map(|v| (v * 100.0).round() as i64)
        .unwrap_or(0);
    let target_date = form.target_date.as_deref().filter(|s| !s.is_empty());

    create_savings_goal(&state.db, &form.name, target, current, target_date, &form.category).await?;
    Ok(axum::response::Redirect::to("/goals").into_response())
}

#[derive(Deserialize)]
pub struct UpdateGoalAmountForm {
    pub amount: String,
}

pub async fn goals_update_amount(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Path(id): Path<String>,
    Form(form): Form<UpdateGoalAmountForm>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    let amount = (form.amount.parse::<f64>().map_err(|_| AppError::BadRequest("Invalid amount".into()))? * 100.0).round() as i64;
    update_savings_goal_amount(&state.db, goal_id, amount).await?;
    Ok(axum::response::Redirect::to("/goals").into_response())
}

pub async fn goals_delete(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let goal_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_savings_goal(&state.db, goal_id).await?;
    Ok(axum::response::Redirect::to("/goals").into_response())
}

// ── Holidays ──

pub async fn holidays(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<impl IntoResponse, AppError> {
    let holidays = list_holidays(&state.db).await?;

    let mut holiday_data = Vec::new();
    for h in &holidays {
        let txns = list_transactions(
            &state.db,
            Some(h.start_date),
            Some(h.end_date),
            None,
            Some("expense"),
        ).await?;
        let total: i64 = txns.iter().map(|t| t.amount.abs()).sum();
        let count = txns.len();
        holiday_data.push((h.clone(), total, count));
    }

    let holiday_rows: String = holiday_data.iter().map(|(h, total, count)| {
        format!(
            "<tr><td>{}</td><td>{} — {}</td><td>{}</td><td>{}</td>\
             <td><form method=\"post\" action=\"/holidays/{}/delete\" style=\"display:inline\" onsubmit=\"return confirm('Delete this holiday?')\">\
             <button type=\"submit\" class=\"btn btn-danger btn-xs\">×</button></form></td></tr>",
            h.name, h.start_date, h.end_date, count, format_cents(*total), h.holiday_id
        )
    }).collect();

    let html = layout("Holidays", maud::html! {
        div class="page-header" {
            h2 { "Holidays & Special Periods" }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Holiday Periods" }
                a href="/holidays/new" class="btn btn-primary btn-sm" { "+ Add Holiday" }
            }
            table class="data-table" {
                thead { tr { th { "Name" } th { "Date Range" } th { "Transactions" } th { "Total Spent" } th {} } }
                tbody { (maud::PreEscaped(holiday_rows)) }
            }
        }
    }, Some(&user), false).into_response();
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
    let html = layout("New Holiday Period", maud::html! {
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
    }, Some(&user), false).into_response();
    Ok(html)
}

pub async fn holidays_create(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<NewHolidayForm>,
) -> Result<impl IntoResponse, AppError> {
    let start = NaiveDate::parse_from_str(&form.start_date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid start date".into()))?;
    let end = NaiveDate::parse_from_str(&form.end_date, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid end date".into()))?;
    if end < start {
        return Err(AppError::BadRequest("End date must be after start date".into()));
    }
    create_holiday(&state.db, &form.name, start, end).await?;
    Ok(axum::response::Redirect::to("/holidays").into_response())
}

pub async fn holidays_delete(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let holiday_id = Uuid::parse_str(&id).map_err(|e| AppError::Internal(e.into()))?;
    delete_holiday(&state.db, holiday_id).await?;
    Ok(axum::response::Redirect::to("/holidays").into_response())
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

    // Get all portfolios and their latest balances
    let portfolios = portfolio::list_portfolios(pool).await?;
    let mut portfolio_data = Vec::new();

    for (pid, pname) in &portfolios {
        let items = portfolio::list_wealth_items(pool, *pid).await?;
        let logs = portfolio::list_balance_logs(pool, *pid).await?;

        // Latest values per item
        let mut latest: std::collections::HashMap<String, (String, String, i64)> = std::collections::HashMap::new();
        for log in &logs {
            let key = log.item_id.to_string();
            let item = items.iter().find(|i| i.item_id == log.item_id).unwrap();
            latest.entry(key).and_modify(|e| {
                if log.log_date.to_string() > e.0 { *e = (log.log_date.to_string(), item.item_type.clone(), log.balance_value); }
            }).or_insert((log.log_date.to_string(), item.item_type.clone(), log.balance_value));
        }

        // Current total (latest values)
        let total_latest: i64 = latest.values().map(|(_, t, v)| if t == "debt" { -*v } else { *v }).sum();

        // Previous month total
        let y: i32 = month[..4].parse().unwrap();
        let m: u32 = month[5..7].parse().unwrap();
        let prev_month = if m == 1 { format!("{}-12", y - 1) } else { format!("{}-{:02}", y, m - 1) };

        let mut prev_values: std::collections::HashMap<String, (String, i64, String)> = std::collections::HashMap::new();
        for log in &logs {
            let key = log.item_id.to_string();
            let date_str = log.log_date.to_string();
            if date_str.starts_with(&prev_month) {
                prev_values.entry(key).and_modify(|e| {
                    if date_str > e.0 { *e = (date_str.clone(), log.balance_value, String::new()); }
                }).or_insert((date_str.clone(), log.balance_value, String::new()));
            }
        }

        let total_prev: i64 = prev_values.values().map(|(_, v, _)| *v).sum::<i64>();
        // If no previous data, use 0
        let total_prev = if prev_values.is_empty() { total_latest } else { total_prev };

        portfolio_data.push((pname.clone(), items.len(), total_latest, total_prev));
    }

    // Get transactions for the month
    let month_start = format!("{}-01", month);
    let month_end = {
        let y: i32 = month[..4].parse().unwrap();
        let m: u32 = month[5..7].parse().unwrap();
        let next = NaiveDate::from_ymd_opt(y, m, 1).unwrap() + chrono::Duration::days(32);
        let last = NaiveDate::from_ymd_opt(next.year(), next.month(), 1).unwrap().pred_opt().unwrap();
        last.to_string()
    };

    let txns = list_transactions(
        pool,
        Some(NaiveDate::parse_from_str(&month_start, "%Y-%m-%d").unwrap()),
        Some(NaiveDate::parse_from_str(&month_end, "%Y-%m-%d").unwrap()),
        None, None,
    ).await?;

    let total_income: i64 = txns.iter().filter(|t| t.amount > 0).map(|t| t.amount).sum();
    let total_expenses: i64 = txns.iter().filter(|t| t.amount < 0).map(|t| t.amount.abs()).sum();
    let net_flow = total_income - total_expenses;

    // Category breakdown
    let mut cat_totals: BTreeMap<String, (i64, i64)> = BTreeMap::new();
    for t in &txns {
        let entry = cat_totals.entry(t.category.clone()).or_insert((0, 0));
        if t.amount > 0 { entry.0 += t.amount; }
        else { entry.1 += t.amount.abs(); }
    }

    let cat_rows: String = cat_totals.iter().map(|(cat, (inc, exp))| {
        format!("<tr><td>{}</td><td class=\"positive\">{}</td><td class=\"negative\">{}</td><td>{}</td></tr>",
            cat, format_cents(*inc), format_cents(*exp), format_cents(*inc as i64 - *exp as i64))
    }).collect();

    let portfolio_rows: String = portfolio_data.iter().map(|(name, items, latest, prev)| {
        let change = *latest as i64 - *prev as i64;
        let cls = if change >= 0 { "positive" } else { "negative" };
        format!("<tr><td>{}</td><td>{}</td><td>{}</td><td>{}</td><td class=\"{}\">{}</td></tr>",
            name, items, format_cents(*prev), format_cents(*latest), cls, format_cents(change))
    }).collect();

    let net_cls = if net_flow >= 0 { "summary-value positive" } else { "summary-value negative" };

    let html = layout("Reconciliation", maud::html! {
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
            table class="data-table" {
                thead { tr { th { "Portfolio" } th { "Items" } th { "Previous" } th { "Current" } th { "Change" } } }
                tbody { (maud::PreEscaped(portfolio_rows)) }
            }
        }
        div class="content-card" {
            div class="content-card-header" {
                h3 { "Spending by Category — " (&month) }
            }
            table class="data-table" {
                thead { tr { th { "Category" } th { "Income" } th { "Expenses" } th { "Net" } } }
                tbody { (maud::PreEscaped(cat_rows)) }
            }
        }
    }, Some(&user), false).into_response();
    Ok(html)
}