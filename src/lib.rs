pub mod auth;
pub mod cookies;
pub mod error;
pub mod flash;
pub mod layout;
pub mod models;
pub mod pages;
pub mod pages_features;
pub mod seed;
pub mod utils;

use axum_extra::extract::cookie::Key;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub key: Key,
}

impl AppState {
    pub fn db(&self) -> &SqlitePool {
        &self.db
    }
}

use axum::Router;

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(pages::hello))
        .route("/time", axum::routing::get(pages::time))
        .route("/signup", axum::routing::get(auth::signup))
        .route("/signup", axum::routing::post(auth::signup_post))
        .route("/login", axum::routing::get(auth::login))
        .route("/login", axum::routing::post(auth::login_post))
        .route("/dashboard", axum::routing::get(pages::dashboard))
        .route("/logout", axum::routing::post(auth::logout_post))
        .route("/portfolios", axum::routing::get(pages::portfolios))
        .route("/portfolios/new", axum::routing::get(pages::new_portfolio_form))
        .route("/portfolios", axum::routing::post(pages::create_portfolio))
        .route("/portfolio/{id}", axum::routing::get(pages::portfolio))
        .route("/portfolio/{id}/items", axum::routing::post(pages::add_item))
        .route("/portfolio/{id}/items/delete", axum::routing::post(pages::delete_item))
        .route("/portfolio/{id}/balances", axum::routing::post(pages::add_balance))
        .route("/portfolio/{id}/balances/delete", axum::routing::post(pages::delete_balance_row))
        .route("/portfolio/{id}/cell", axum::routing::get(pages::edit_cell))
        .route("/portfolio/{id}/cell", axum::routing::put(pages::save_cell))
        .route("/portfolio/{id}/delete", axum::routing::post(pages::delete_portfolio))
        .route("/portfolio/{id}/import", axum::routing::get(pages::portfolio_import))
        .route("/portfolio/{id}/import", axum::routing::post(pages::portfolio_import_post))
        .route("/portfolio/{id}/export/csv", axum::routing::get(pages::portfolio_csv))
        .route("/stats", axum::routing::get(pages::stats))
        // Feature pages
        .route("/transactions", axum::routing::get(pages_features::transactions))
        .route("/transactions/new", axum::routing::get(pages_features::transactions_new))
        .route("/transactions/new", axum::routing::post(pages_features::transactions_create))
        .route("/transactions/{id}", axum::routing::post(pages_features::transactions_delete))
        .route("/transactions/{id}/edit", axum::routing::get(pages_features::transactions_edit))
        .route("/transactions/{id}/edit", axum::routing::post(pages_features::transactions_update))
        .route("/transactions/export/csv", axum::routing::get(pages_features::transactions_csv))
        .route("/budgets", axum::routing::get(pages_features::budgets))
        .route("/budgets/new", axum::routing::get(pages_features::budgets_new))
        .route("/budgets/new", axum::routing::post(pages_features::budgets_create))
        .route("/budgets/{id}/edit", axum::routing::get(pages_features::budgets_edit))
        .route("/budgets/{id}/edit", axum::routing::post(pages_features::budgets_update))
        .route("/budgets/{id}/delete", axum::routing::post(pages_features::budgets_delete))
        .route("/goals", axum::routing::get(pages_features::goals))
        .route("/goals/new", axum::routing::get(pages_features::goals_new))
        .route("/goals/new", axum::routing::post(pages_features::goals_create))
        .route("/goals/{id}/update", axum::routing::post(pages_features::goals_update_amount))
        .route("/goals/{id}/edit", axum::routing::get(pages_features::goals_edit))
        .route("/goals/{id}/edit", axum::routing::post(pages_features::goals_update))
        .route("/goals/{id}/delete", axum::routing::post(pages_features::goals_delete))
        .route("/holidays", axum::routing::get(pages_features::holidays))
        .route("/holidays/new", axum::routing::get(pages_features::holidays_new))
        .route("/holidays/new", axum::routing::post(pages_features::holidays_create))
        .route("/holidays/{id}/delete", axum::routing::post(pages_features::holidays_delete))
        .route("/holidays/{id}/edit", axum::routing::get(pages_features::holidays_edit))
        .route("/holidays/{id}/edit", axum::routing::post(pages_features::holidays_update))
        .route("/reconciliation", axum::routing::get(pages_features::reconciliation))
        .nest_service("/static", tower_http::services::ServeDir::new("src/static"))
        .fallback(pages::not_found)
        .with_state(state)
}