use rust_web::AppState;
use rust_web::auth;
use rust_web::models::backup::LitestreamGuard;
use rust_web::pages;
use std::str::FromStr;

use axum::Router;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
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
    let config_dir = std::env::var("CONFIG_DIR").unwrap_or_else(|_| ".".to_string());
    tracing::info!("database: {connection_string}, db_path: {db_path}, config_dir: {config_dir}");
    let options = SqliteConnectOptions::from_str(&connection_string)?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!().run(&db).await?;

    let key = axum_extra::extract::cookie::Key::generate();
    let litestream_child = Arc::new(Mutex::new(None));
    // RAII guard: even if we exit via panic or error, the litestream child
    // process will be killed when this is dropped.
    let _litestream_guard = LitestreamGuard::new(litestream_child.clone());
    let state = AppState {
        db: Arc::new(RwLock::new(db.clone())),
        key,
        db_path: db_path.clone(),
        config_dir: config_dir.clone(),
        litestream_child: litestream_child.clone(),
    };

    let static_dir = std::env::var("STATIC_DIR").unwrap_or_else(|_| "src/static".to_string());

    // If backups are enabled, start litestream on startup
    if let Err(e) =
        rust_web::models::backup::sync_litestream(&db, &db_path, &config_dir, &litestream_child)
            .await
    {
        tracing::warn!("Failed to sync litestream on startup: {e:?}");
    }

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await?;
    tracing::info!("listening on {}", listener.local_addr().unwrap());

    let litestream_for_shutdown = litestream_child.clone();
    axum::serve(listener, app(state, static_dir))
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            tracing::info!("Shutting down...");
            // Kill litestream on shutdown so it doesn't outlive the app
            rust_web::models::backup::stop_litestream(&litestream_for_shutdown).await;
        })
        .await?;

    Ok(())
}

fn app(state: AppState, static_dir: String) -> Router {
    Router::new()
        .route("/", axum::routing::get(pages::hello))
        .route("/time", axum::routing::get(pages::time))
        .route("/signup", axum::routing::get(auth::signup))
        .route("/signup", axum::routing::post(auth::signup_post))
        .route("/login", axum::routing::get(auth::login))
        .route("/login", axum::routing::post(auth::login_post))
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
