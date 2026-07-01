mod auth;
mod cookies;
mod error;
mod layout;
mod models;
mod pages;
mod utils;
use std::str::FromStr;

use axum::Router;
use axum_extra::extract::cookie::Key;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use tower_http::services::ServeDir;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rust_web=debug".parse().unwrap()),
        )
        .init();

    let connection_string = "sqlite://data.db";
    let options = SqliteConnectOptions::from_str(connection_string)?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!().run(&db).await?;

    let key = Key::generate();
    let state = crate::AppState { db, key };

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app(state)).await?;

    Ok(())
}

#[derive(Clone)]
pub struct AppState {
    db: SqlitePool,
    pub key: Key,
}

impl AppState {
    pub fn db(&self) -> &SqlitePool {
        &self.db
    }
}

fn app(state: AppState) -> Router {
    Router::new()
        .route("/", axum::routing::get(pages::hello))
        .route("/time", axum::routing::get(pages::time))
        .route("/signup", axum::routing::get(auth::signup))
        .route("/signup", axum::routing::post(auth::signup_post))
        .route("/login", axum::routing::get(auth::login))
        .route("/login", axum::routing::post(auth::login_post))
        .route("/dashboard", axum::routing::get(pages::dashboard))
        .route("/insights", axum::routing::get(pages::insights))
        .route("/insights/{id}", axum::routing::get(pages::insights_chart))
        .route("/logout", axum::routing::post(auth::logout_post))
        .route("/portfolios", axum::routing::get(pages::portfolios))
        .route("/portfolios", axum::routing::post(pages::create_portfolio))
        .route("/portfolio/{id}", axum::routing::get(pages::portfolio))
        .route(
            "/portfolio/{id}/items",
            axum::routing::post(pages::add_item),
        )
        .route(
            "/portfolio/{id}/rename",
            axum::routing::post(pages::rename_portfolio),
        )
        .route(
            "/portfolio/{id}/balances",
            axum::routing::post(pages::add_balance),
        )
        .route("/portfolio/{id}/cell", axum::routing::get(pages::edit_cell))
        .route("/portfolio/{id}/cell", axum::routing::put(pages::save_cell))
        .route("/portfolio/{id}/date", axum::routing::get(pages::edit_date))
        .route("/portfolio/{id}/date", axum::routing::put(pages::save_date))
        .route("/portfolio/{id}/row", axum::routing::get(pages::get_row))
        .route(
            "/portfolio/{id}/rename-item",
            axum::routing::post(pages::save_item_name),
        )
        .route(
            "/portfolio/{id}/move-item",
            axum::routing::post(pages::move_item),
        )
        .route(
            "/portfolio/{id}/change-type",
            axum::routing::post(pages::change_item_type),
        )
        .route(
            "/portfolio/{id}/delete-item",
            axum::routing::post(pages::delete_item),
        )
        .route("/reconcile", axum::routing::get(pages::reconcile_list))
        .route("/reconcile", axum::routing::post(pages::reconcile_create))
        .route("/reconcile/{id}", axum::routing::get(pages::reconcile_detail))
        .route("/reconcile/{id}/delete", axum::routing::post(pages::reconcile_delete))
        .route("/reconcile/{id}/rename", axum::routing::post(pages::rename_session))
        .route("/reconcile/{id}/outgoing", axum::routing::post(pages::add_outgoing))
        .route("/reconcile/{id}/outgoing/csv", axum::routing::post(pages::upload_outgoing_csv))
        .route("/reconcile/{id}/reconciled", axum::routing::post(pages::add_reconciled))
        .route("/reconcile/{id}/reconciled/csv", axum::routing::post(pages::upload_reconciled_csv))
        .route("/reconcile/{id}/link", axum::routing::post(pages::link_txns))
        .route("/reconcile/{id}/unlink", axum::routing::post(pages::unlink_txns))
        .route("/reconcile/{id}/unlink-reconciled", axum::routing::post(pages::unlink_reconciled_txns))
        .route("/reconcile/{id}/auto-match", axum::routing::post(pages::auto_match))
        .nest_service("/static", ServeDir::new("src/static"))
        .fallback(pages::not_found)
        .with_state(state)
}
