use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::portfolio;
use crate::utils;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use chrono::NaiveDate;
use uuid::Uuid;

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
                details {
                    summary { "+ Add Balance Row" }
                    form method="post" action=(format!("/portfolio/{}/balances", portfolio_id)) {
                        label { "Date"
                            input type="date" name="log_date" required {}
                        }
                        @for item in &items {
                            label { (item.name) input type="number" step="0.01"
                                name=(format!("balance_{}", item.item_id))
                            placeholder="$0.00" {}
                        }
                    }
                            button type="submit" { "Save Row" }
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
) -> Result<axum::response::Redirect, AppError> {
    portfolio::get_portfolio(state.db(), portfolio_id, user.0).await?;

    let log_date_str = form
        .get("log_date")
        .ok_or_else(|| AppError::BadRequest("Missing log date field".into()))?;
    let log_date = NaiveDate::parse_from_str(log_date_str, "%Y-%m-%d")?;
    let items = portfolio::list_wealth_items(state.db(), portfolio_id).await?;
    for item in &items {
        let key = format!("balance_{}", item.item_id);
        if let Some(value) = form.get(&key) {
            if let Ok(cents) = utils::parse_dollars(value) {
                portfolio::insert_balance_log(state.db(), item.item_id, log_date, cents).await?;
            }
        }
    }
    Ok(axum::response::Redirect::to(&format!(
        "/portfolio/{}",
        portfolio_id
    )))
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
