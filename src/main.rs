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
        .route("/logout", axum::routing::post(auth::logout_post))
        .route("/portfolios", axum::routing::get(pages::portfolios))
        .route("/portfolios", axum::routing::post(pages::create_portfolio))
        .route("/portfolio/{id}", axum::routing::get(pages::portfolio))
        .route("/portfolio/{id}/items", axum::routing::post(pages::add_item))
        .route("/portfolio/{id}/balances", axum::routing::post(pages::add_balance))
        .route("/portfolio/{id}/cell", axum::routing::get(pages::edit_cell))
        .route("/portfolio/{id}/cell", axum::routing::put(pages::save_cell))
        .nest_service("/static", ServeDir::new("src/static"))
        .fallback(pages::not_found)
        .with_state(state)
}
