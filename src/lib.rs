pub mod auth;
pub mod cookies;
pub mod error;
pub mod layout;
pub mod models {
    pub mod backup;
    pub mod csv_import;
    pub mod portfolio;
    pub mod reconcile;
    pub mod user;
}
pub mod pages;
pub mod utils;

use axum_extra::extract::cookie::Key;
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub key: Key,
    pub db_path: String,
}

impl AppState {
    pub fn db(&self) -> &SqlitePool {
        &self.db
    }
}
