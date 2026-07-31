use financials::AppState;
use financials::auth;
use financials::handlers;
use financials::models::user;
use financials::views;
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
                .add_directive("financials=debug".parse().unwrap()),
        )
        .init();

    let connection_string =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://data.db".to_string());
    let db_path = std::env::var("DB_PATH").unwrap_or_else(|_| "data.db".to_string());

    tracing::info!("database: {connection_string}, db_path: {db_path}");
    let options = SqliteConnectOptions::from_str(&connection_string)?.create_if_missing(true);
    let db = SqlitePool::connect_with(options).await?;
    sqlx::migrate!().run(&db).await?;

    // Admin credentials from env vars (optional — on first boot the user sets
    // their own password via the change-password flow).
    let admin_username = std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let admin_password_hash = if let Ok(hash) = std::env::var("ADMIN_PASSWORD_HASH") {
        hash
    } else if let Ok(plain) = std::env::var("ADMIN_PASSWORD") {
        bcrypt::hash(&plain, bcrypt::DEFAULT_COST)?
    } else {
        // No password configured — store empty string. The user will be
        // prompted to set a password on first login.
        String::new()
    };

    // Seed the admin user (create or update password)
    let (admin_user_id, stored_hash) = user::seed_admin(&db, &admin_username, &admin_password_hash)
        .await
        .map_err(|e| anyhow::anyhow!("failed to seed admin: {e:?}"))?;
    tracing::info!("Admin user '{admin_username}' ready (id={admin_user_id})");

    let key = axum_extra::extract::cookie::Key::generate();
    let secure_cookies = std::env::var("SECURE_COOKIES")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);
    let state = AppState {
        db: Arc::new(RwLock::new(db.clone())),
        key,
        db_path: db_path.clone(),
        admin_password_hash: Arc::new(std::sync::RwLock::new(stored_hash)),
        admin_username: admin_username.clone(),
        admin_user_id: Arc::new(std::sync::RwLock::new(admin_user_id)),
        secure_cookies,
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
                let config = match financials::models::backup::get_config(&pool).await {
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
                let config = match financials::models::backup::get_config(&pool).await {
                    Ok(Some(c)) if c.enabled => c,
                    _ => continue,
                };

                tracing::info!(
                    "Automatic snapshot: creating (interval={}min)",
                    config.interval_minutes
                );
                match financials::models::backup::create_snapshot(&pool, &db_path_inner, &config)
                    .await
                {
                    Ok(key) => tracing::info!("Automatic snapshot created: {key}"),
                    Err(e) => tracing::error!("Automatic snapshot failed: {e:?}"),
                }
            }
        });
    }

    let listener = {
        let mut port = 3000u16;
        loop {
            let addr = format!("0.0.0.0:{port}");
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(l) => break l,
                Err(e) if port < 3100 => {
                    tracing::warn!("{addr} unavailable ({e}), trying port {}", port + 1);
                    port += 1;
                }
                Err(e) => anyhow::bail!("no available port from 3000..3100: {e}"),
            }
        }
    };
    tracing::info!("listening on {}", listener.local_addr().unwrap());

    // Open the browser at the local server URL as a convenience.
    let url = format!("http://localhost:{}", listener.local_addr().unwrap().port());
    if let Err(e) = webbrowser::open(&url) {
        tracing::warn!("failed to open browser: {e}");
    }

    axum::serve(listener, app(state, static_dir))
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

fn app(state: AppState, static_dir: String) -> Router {
    Router::new()
        // Public routes (no auth required)
        .route("/", axum::routing::get(handlers::hello::hello))
        .route("/time", axum::routing::get(handlers::hello::time))
        .route("/login", axum::routing::get(auth::login))
        .route("/login", axum::routing::post(auth::login_post))
        .route("/backup", axum::routing::get(views::settings::backup_page))
        .route(
            "/backup/restore",
            axum::routing::post(handlers::backup::backup_restore),
        )
        .route(
            "/backup/restore-points",
            axum::routing::get(handlers::backup::backup_restore_points),
        )
        // Authenticated routes
        .route("/dashboard", axum::routing::get(views::settings::dashboard))
        .route("/settings", axum::routing::get(views::settings::settings))
        .route(
            "/settings/backup",
            axum::routing::post(handlers::backup::settings_backup_post),
        )
        .route(
            "/settings/backup/enable",
            axum::routing::post(handlers::backup::settings_backup_enable),
        )
        .route(
            "/settings/backup/disable",
            axum::routing::post(handlers::backup::settings_backup_disable),
        )
        .route(
            "/settings/backup/restore",
            axum::routing::post(handlers::backup::settings_backup_restore),
        )
        .route(
            "/settings/backup/restore-points",
            axum::routing::get(handlers::backup::settings_backup_restore_points),
        )
        .route(
            "/settings/backup/snapshot",
            axum::routing::post(handlers::backup::settings_backup_snapshot),
        )
        .route(
            "/backup/configure",
            axum::routing::post(handlers::backup::backup_configure),
        )
        .route(
            "/backup/enable",
            axum::routing::post(handlers::backup::backup_enable),
        )
        .route(
            "/backup/disable",
            axum::routing::post(handlers::backup::backup_disable),
        )
        .route(
            "/backup/snapshot",
            axum::routing::post(handlers::backup::backup_snapshot),
        )
        .route("/insights", axum::routing::get(views::settings::insights))
        .route(
            "/insights/{id}",
            axum::routing::get(views::settings::insights_chart),
        )
        .route("/logout", axum::routing::post(auth::logout_post))
        .route(
            "/setup-password",
            axum::routing::post(auth::setup_password_post),
        )
        .route(
            "/change-password",
            axum::routing::get(auth::change_password_get),
        )
        .route(
            "/change-password",
            axum::routing::post(auth::change_password_post),
        )
        .route(
            "/portfolios",
            axum::routing::get(views::portfolio::portfolios),
        )
        .route(
            "/portfolios",
            axum::routing::post(handlers::create_portfolio::create_portfolio),
        )
        .route(
            "/portfolio/{id}",
            axum::routing::get(views::portfolio::portfolio),
        )
        .route(
            "/portfolio/{id}/items",
            axum::routing::post(handlers::items::add_item),
        )
        .route(
            "/portfolio/{id}/rename",
            axum::routing::post(handlers::rename_portfolio::rename_portfolio),
        )
        .route(
            "/portfolio/{id}/balances",
            axum::routing::post(handlers::portfolio::add_balance),
        )
        .route(
            "/portfolio/{id}/cell",
            axum::routing::get(handlers::portfolio::edit_cell),
        )
        .route(
            "/portfolio/{id}/cell",
            axum::routing::put(handlers::portfolio::save_cell),
        )
        .route(
            "/portfolio/{id}/date",
            axum::routing::get(handlers::portfolio::edit_date),
        )
        .route(
            "/portfolio/{id}/date",
            axum::routing::put(handlers::portfolio::save_date),
        )
        .route(
            "/portfolio/{id}/row",
            axum::routing::get(handlers::portfolio::get_row),
        )
        .route(
            "/portfolio/{id}/rename-item",
            axum::routing::post(handlers::items::save_item_name),
        )
        .route(
            "/portfolio/{id}/move-item",
            axum::routing::post(handlers::items::move_item),
        )
        .route(
            "/portfolio/{id}/change-type",
            axum::routing::post(handlers::items::change_item_type),
        )
        .route(
            "/portfolio/{id}/delete-item",
            axum::routing::post(handlers::items::delete_item),
        )
        .route(
            "/portfolio/{id}/import",
            axum::routing::get(handlers::portfolio::portfolio_import),
        )
        .route(
            "/portfolio/{id}/import",
            axum::routing::post(handlers::portfolio::portfolio_import_post),
        )
        .route(
            "/portfolio/{id}/import/confirm",
            axum::routing::post(handlers::portfolio::portfolio_import_confirm),
        )
        .route(
            "/portfolio/{id}/export/csv",
            axum::routing::get(handlers::portfolio::portfolio_csv),
        )
        .route(
            "/reconcile",
            axum::routing::get(views::reconcile::reconcile_list),
        )
        .route(
            "/reconcile",
            axum::routing::post(handlers::reconcile::reconcile_create),
        )
        .route(
            "/reconcile/{id}",
            axum::routing::get(views::reconcile::reconcile_detail),
        )
        .route(
            "/reconcile/{id}/delete",
            axum::routing::post(handlers::reconcile::reconcile_delete),
        )
        .route(
            "/reconcile/{id}/rename",
            axum::routing::post(handlers::reconcile::rename_session),
        )
        .route(
            "/reconcile/{id}/outgoing",
            axum::routing::post(handlers::reconcile::add_outgoing),
        )
        .route(
            "/reconcile/{id}/outgoing/csv",
            axum::routing::post(handlers::reconcile::upload_outgoing_csv),
        )
        .route(
            "/reconcile/{id}/outgoing-csv/confirm",
            axum::routing::post(handlers::reconcile::confirm_outgoing_csv),
        )
        .route(
            "/reconcile/{id}/reconciled",
            axum::routing::post(handlers::reconcile::add_reconciled),
        )
        .route(
            "/reconcile/{id}/reconciled/csv",
            axum::routing::post(handlers::reconcile::upload_reconciled_csv),
        )
        .route(
            "/reconcile/{id}/reconciled-csv/confirm",
            axum::routing::post(handlers::reconcile::confirm_reconciled_csv),
        )
        .route(
            "/reconcile/{id}/link",
            axum::routing::post(handlers::reconcile::link_txns),
        )
        .route(
            "/reconcile/{id}/unlink",
            axum::routing::post(handlers::reconcile::unlink_txns),
        )
        .route(
            "/reconcile/{id}/unlink-reconciled",
            axum::routing::post(handlers::reconcile::unlink_reconciled_txns),
        )
        .route(
            "/reconcile/{id}/auto-match",
            axum::routing::post(handlers::reconcile::auto_match),
        )
        .route(
            "/reconcile/{id}/ignore-outgoing/{txn_id}",
            axum::routing::post(handlers::reconcile::ignore_outgoing),
        )
        .route(
            "/reconcile/{id}/ignore-reconciled/{txn_id}",
            axum::routing::post(handlers::reconcile::ignore_reconciled),
        )
        .route(
            "/reconcile/{id}/unignore-outgoing/{txn_id}",
            axum::routing::post(handlers::reconcile::unignore_outgoing),
        )
        .route(
            "/reconcile/{id}/unignore-reconciled/{txn_id}",
            axum::routing::post(handlers::reconcile::unignore_reconciled),
        )
        .route(
            "/reconcile/{id}/confirm",
            axum::routing::post(handlers::reconcile::confirm_proposal),
        )
        .route(
            "/reconcile/{id}/confirm-all",
            axum::routing::post(handlers::reconcile::confirm_all_proposals),
        )
        .route(
            "/reconcile/{id}/confirm-exact",
            axum::routing::post(handlers::reconcile::confirm_exact_proposals),
        )
        .route(
            "/reconcile/{id}/reject",
            axum::routing::post(handlers::reconcile::reject_proposal),
        )
        .route(
            "/reconcile/{id}/undo-reject/{outgoing_id}",
            axum::routing::post(handlers::reconcile::undo_reject),
        )
        .nest_service("/static", ServeDir::new(static_dir))
        .fallback(handlers::hello::not_found)
        .with_state(state)
}
