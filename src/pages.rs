use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::layout::layout;
use crate::models::portfolio::list_portfolios;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
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

pub async fn portfolios(
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let portfolios = list_portfolios(state.db(), user.0).await?;
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
    Path(portfoio_id): Path<Uuid>,
    State(state): State<AppState>,
    user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    todo!()
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
