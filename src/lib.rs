pub mod auth;
pub mod cookies;
pub mod error;
pub mod layout;
pub mod models;
pub mod pages;
pub mod utils;

use axum_extra::extract::cookie::Key;
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<RwLock<SqlitePool>>,
    pub key: Key,
    pub db_path: String,
    /// Wrapped in RwLock so it can be updated at runtime (password change).
    pub admin_password_hash: Arc<std::sync::RwLock<String>>,
    pub admin_username: String,
    /// Wrapped in RwLock so it can be updated after a DB restore
    /// (the restored DB may have a different admin user ID).
    pub admin_user_id: Arc<std::sync::RwLock<Uuid>>,
    /// When true, session cookies get Secure + SameSite=Lax flags.
    /// Set via SECURE_COOKIES=true env var. Defaults to false so
    /// home-lab HTTP deployments work out of the box.
    pub secure_cookies: bool,
}

impl AppState {
    /// Get a clone of the current SqlitePool.
    /// SqlitePool is cheaply cloneable (Arc-based internally).
    pub async fn db(&self) -> SqlitePool {
        self.db.read().await.clone()
    }

    /// Swap the database pool in-place (used by restore).
    pub async fn swap_db(&self, new_pool: SqlitePool) {
        *self.db.write().await = new_pool;
    }
}
