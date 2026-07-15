use rust_web::AppState;
use rust_web::auth;
use rust_web::models::user;
use rust_web::pages;
use std::str::FromStr;

use axum::Router;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_http::services::ServeDir;

/// Listen for both SIGINT (ctrl-c) and SIGTERM (Docker stop / kill) so the
/// graceful-shutdown handler runs in all normal termination scenarios.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl+c");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("rust_web=debug".parse().unwrap()),
        )
        .init();

    let connection_string =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data.db".to_string());
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "data.db".to_string());

    tracing::info!("database: {connection_string}, db_path: {db_path}");
    let options = SqliteConnectOptions::from_str(&connection_string)?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!().run(&db).await?;

    // Admin credentials from env vars.
    // ADMIN_USERNAME defaults to "admin", ADMIN_PASSWORD defaults to "admin".
    // For production, set ADMIN_PASSWORD_HASH to a bcrypt hash instead.
    let admin_username = std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let admin_password_hash = if let Ok(hash) = std::env::var("ADMIN_PASSWORD_HASH") {
        hash
    } else {
        let plain = std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());
        bcrypt::hash(&plain, bcrypt::DEFAULT_COST)?
    };

    // Seed the admin user (create or update password)
    let admin_user_id = user::seed_admin(&db, &admin_username, &admin_password_hash)
        .await
        .map_err(|e| anyhow::anyhow!("failed to seed admin: {e:?}"))?;
    tracing::info!("Admin user '{admin_username}' ready (id={admin_user_id})");

    let key = axum_extra::extract::cookie::Key::generate();
    let state = AppState {
        db: Arc::new(RwLock::new(db.clone())),
        key,
        db_path: db_path.clone(),
        admin_password_hash,
        admin_username: admin_username.clone(),
        admin_user_id: Arc::new(std::sync::RwLock::new(admin_user_id)),
    };

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "src/static".to_string());

    // Spawn background snapshot scheduler
    {
        let db_inner = state.db.clone();
        let db_path_inner = state.db_path.clone();
        tokio::spawn(async move {
            // Initial delay to let the server settle
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;

            loop {
                // Read current config from DB
                let pool = db_inner.read().await.clone();
                let config = match rust_web::models::backup::get_config(&pool).await {
                    Ok(Some(c)) => c,
                    _ => {
                        // No config yet — check again in 5 minutes
                        tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                        continue;
                    }
                };

                if !config.enabled {
                    // Backups disabled — check again in 5 minutes
                    tokio::time::sleep(tokio::time::Duration::from_secs(300)).await;
                    continue;
                }

                let interval =
                    tokio::time::Duration::from_secs(config.interval_minutes.max(5) as u64 * 60);
                tokio::time::sleep(interval).await;

                // Re-read pool and config (may have changed during sleep)
                let pool = db_inner.read().await.clone();
                let config = match rust_web::models::backup::get_config(&pool).await {
                    Ok(Some(c)) if c.enabled => c,
                    _ => continue,
                };

                tracing::info!(
                    "Automatic snapshot: creating (interval={}min)",
                    config.interval_minutes
                );
                match rust_web::models::backup::create_snapshot(&pool, &db_path_inner, &config)
                    .await
                {
                    Ok(key) => tracing::info!("Automatic snapshot created: {key}"),
                    Err(e) => tracing::error!("Automatic snapshot failed: {e:?}"),
                }
            }
        });
    }

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on {}", listener.local_addr().unwrap());

    axum::serve(listener, app(state, static_dir))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn app(state: AppState, static_dir: String) -> Router {
    Router::new()
        // Public routes (no auth required)
        .route("/", axum::routing::get(pages::hello))
        .route("/time", axum::routing::get(pages::time))
        .route("/login", axum::routing::get(auth::login))
        .route("/login", axum::routing::post(auth::login_post))
        .route("/backup", axum::routing::get(pages::backup_page))
        .route(
            "/backup/configure",
            axum::routing::post(pages::backup_configure),
        )
        .route("/backup/enable", axum::routing::post(pages::backup_enable))
        .route(
            "/backup/disable",
            axum::routing::post(pages::backup_disable),
        )
        .route(
            "/backup/restore",
            axum::routing::post(pages::backup_restore),
        )
        .route(
            "/backup/restore-points",
            axum::routing::get(pages::backup_restore_points),
        )
        .route(
            "/backup/snapshot",
            axum::routing::post(pages::backup_snapshot),
        )
        // Authenticated routes
        .route("/dashboard", axum::routing::get(pages::dashboard))
        .route("/settings", axum::routing::get(pages::settings))
        .route(
            "/settings/backup",
            axum::routing::post(pages::settings_backup_post),
        )
        .route(
            "/settings/backup/enable",
            axum::routing::post(pages::settings_backup_enable),
        )
        .route(
            "/settings/backup/disable",
            axum::routing::post(pages::settings_backup_disable),
        )
        .route(
            "/settings/backup/restore",
            axum::routing::post(pages::settings_backup_restore),
        )
        .route(
            "/settings/backup/restore-points",
            axum::routing::get(pages::settings_backup_restore_points),
        )
        .route(
            "/settings/backup/snapshot",
            axum::routing::post(pages::settings_backup_snapshot),
        )
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
        .route(
            "/portfolio/{id}/import",
            axum::routing::get(pages::portfolio_import),
        )
        .route(
            "/portfolio/{id}/import",
            axum::routing::post(pages::portfolio_import_post),
        )
        .route(
            "/portfolio/{id}/import/confirm",
            axum::routing::post(pages::portfolio_import_confirm),
        )
        .route(
            "/portfolio/{id}/export/csv",
            axum::routing::get(pages::portfolio_csv),
        )
        .route("/reconcile", axum::routing::get(pages::reconcile_list))
        .route("/reconcile", axum::routing::post(pages::reconcile_create))
        .route(
            "/reconcile/{id}",
            axum::routing::get(pages::reconcile_detail),
        )
        .route(
            "/reconcile/{id}/delete",
            axum::routing::post(pages::reconcile_delete),
        )
        .route(
            "/reconcile/{id}/rename",
            axum::routing::post(pages::rename_session),
        )
        .route(
            "/reconcile/{id}/outgoing",
            axum::routing::post(pages::add_outgoing),
        )
        .route(
            "/reconcile/{id}/outgoing/csv",
            axum::routing::post(pages::upload_outgoing_csv),
        )
        .route(
            "/reconcile/{id}/outgoing-csv/confirm",
            axum::routing::post(pages::confirm_outgoing_csv),
        )
        .route(
            "/reconcile/{id}/reconciled",
            axum::routing::post(pages::add_reconciled),
        )
        .route(
            "/reconcile/{id}/reconciled/csv",
            axum::routing::post(pages::upload_reconciled_csv),
        )
        .route(
            "/reconcile/{id}/reconciled-csv/confirm",
            axum::routing::post(pages::confirm_reconciled_csv),
        )
        .route(
            "/reconcile/{id}/link",
            axum::routing::post(pages::link_txns),
        )
        .route(
            "/reconcile/{id}/unlink",
            axum::routing::post(pages::unlink_txns),
        )
        .route(
            "/reconcile/{id}/unlink-reconciled",
            axum::routing::post(pages::unlink_reconciled_txns),
        )
        .route(
            "/reconcile/{id}/auto-match",
            axum::routing::post(pages::auto_match),
        )
        .route(
            "/reconcile/{id}/ignore-outgoing/{txn_id}",
            axum::routing::post(pages::ignore_outgoing),
        )
        .route(
            "/reconcile/{id}/ignore-reconciled/{txn_id}",
            axum::routing::post(pages::ignore_reconciled),
        )
        .route(
            "/reconcile/{id}/confirm",
            axum::routing::post(pages::confirm_proposal),
        )
        .route(
            "/reconcile/{id}/confirm-all",
            axum::routing::post(pages::confirm_all_proposals),
        )
        .route(
            "/reconcile/{id}/reject",
            axum::routing::post(pages::reject_proposal),
        )
        .nest_service("/static", ServeDir::new(static_dir))
        .fallback(pages::not_found)
        .with_state(state)
}
