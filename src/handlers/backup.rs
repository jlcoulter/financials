use crate::AppState;
use crate::cookies::LoggedInUser;
use crate::error::AppError;
use crate::models::backup;
use crate::models::user;
use crate::requests::{BackupForm, PublicBackupForm, RestoreForm};
use axum::extract::{Form, State};
use axum::response::{IntoResponse, Redirect};
use uuid::Uuid;

// ── Settings backup handlers (authenticated) ──

pub async fn settings_backup_post(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<BackupForm>,
) -> Result<axum::response::Response, AppError> {
    // Trim empty strings to None for optional fields
    let endpoint = form.endpoint.filter(|s| !s.trim().is_empty());
    let access_key_id = form.access_key_id.filter(|s| !s.trim().is_empty());
    let secret_access_key = form.secret_access_key.filter(|s| !s.trim().is_empty());
    let b2_key_id = form.b2_key_id.filter(|s| !s.trim().is_empty());
    let b2_application_key = form.b2_application_key.filter(|s| !s.trim().is_empty());
    let b2_endpoint = form.b2_endpoint.filter(|s| !s.trim().is_empty());

    // Pick bucket/path/region based on provider
    let (bucket, path, region) = if form.provider == "b2" {
        (
            form.b2_bucket
                .clone()
                .unwrap_or_else(|| form.bucket.clone()),
            form.b2_path.clone().unwrap_or_else(|| form.path.clone()),
            form.b2_region
                .clone()
                .unwrap_or_else(|| form.region.clone()),
        )
    } else {
        (form.bucket.clone(), form.path.clone(), form.region.clone())
    };

    // If secret_access_key is empty and we have an existing config, keep the old one
    let secret_access_key = match secret_access_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(&state.db().await).await?;
            existing.and_then(|c| c.secret_access_key)
        }
    };
    let b2_application_key = match b2_application_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(&state.db().await).await?;
            existing.and_then(|c| c.b2_application_key)
        }
    };

    let config = backup::BackupConfig {
        id: Uuid::nil(), // Will be set by save_config if new
        provider: form.provider,
        bucket,
        path,
        region,
        endpoint,
        access_key_id,
        secret_access_key,
        b2_key_id,
        b2_application_key,
        b2_endpoint,
        enabled: false,       // Start paused; user explicitly enables
        db_instance_id: None, // Will be assigned by save_config
        interval_minutes: form.interval_minutes.unwrap_or(60),
        max_snapshots: form.max_snapshots.unwrap_or(10),
    };

    // Preserve existing enabled state if updating
    let existing = backup::get_config(&state.db().await).await?;
    let config = match existing {
        Some(mut c) => {
            c.provider = config.provider;
            c.bucket = config.bucket;
            c.path = config.path;
            c.region = config.region;
            c.endpoint = config.endpoint;
            c.access_key_id = config.access_key_id;
            c.secret_access_key = config.secret_access_key;
            c.b2_key_id = config.b2_key_id;
            c.b2_application_key = config.b2_application_key;
            c.b2_endpoint = config.b2_endpoint;
            c
        }
        None => config,
    };

    backup::save_config(&state.db().await, &config).await?;

    // If the config is enabled, create a snapshot immediately
    if config.enabled {
        let pool = state.db().await.clone();
        if let Err(e) = backup::create_snapshot(&pool, &state.db_path, &config).await {
            tracing::warn!("Failed to create snapshot after saving config: {e:?}");
        }
    }

    Ok(Redirect::to("/settings?flash=saved").into_response())
}

pub async fn settings_backup_enable(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(&state.db().await, true).await?;
    // Create a snapshot now that backups are enabled
    if let Some(config) = backup::get_config(&state.db().await).await? {
        let pool = state.db().await.clone();
        if let Err(e) = backup::create_snapshot(&pool, &state.db_path, &config).await {
            tracing::warn!("Failed to create snapshot on enable: {e:?}");
        }
    }
    Ok(Redirect::to("/settings?flash=enabled").into_response())
}

pub async fn settings_backup_disable(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(&state.db().await, false).await?;
    Ok(Redirect::to("/settings?flash=disabled").into_response())
}

pub async fn settings_backup_restore(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<RestoreForm>,
) -> Result<axum::response::Response, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    // For restore, the "timestamp" field now holds the full snapshot key
    let snapshot_key = form.timestamp.as_deref().unwrap_or("");

    match backup::restore_from_snapshot(&state.db, &state.db_path, &config, snapshot_key).await {
        Ok(()) => {
            tracing::info!("Restore complete, pool reconnected");
            // Re-seed admin user so the in-memory admin_user_id matches
            // the restored DB (which may have a different user ID).
            let pool = state.db().await;
            let current_hash = state.admin_password_hash.read().unwrap().clone();
            let (new_admin_id, _) = user::seed_admin(&pool, &state.admin_username, &current_hash)
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!(
                        "failed to re-seed admin after restore: {e:?}"
                    ))
                })?;
            *state.admin_user_id.write().unwrap() = new_admin_id;
            drop(pool);
            tracing::info!("Admin user re-synced after restore (id={new_admin_id})");
            // Redirect to login — the old session cookie has a stale user_id
            // that may not exist in the restored DB, so the user must re-authenticate.
            Ok(Redirect::to("/login?flash=restored").into_response())
        }
        Err(e) => {
            tracing::error!("Restore failed: {e:?}");
            Ok(Redirect::to("/settings?flash=restore_failed").into_response())
        }
    }
}

/// HTMX endpoint: returns the snapshot dropdown HTML asynchronously.
pub async fn settings_backup_restore_points(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<maud::Markup, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    let snapshots = backup::list_all_snapshots(&config)
        .await
        .unwrap_or_default();

    Ok(maud::html! {
        select name="timestamp" {
            @for snapshot in &snapshots {
                @let size_kb = snapshot.size as f64 / 1024.0;
                @let size_str = if size_kb >= 1024.0 {
                    format!("{:.1} MB", size_kb / 1024.0)
                } else {
                    format!("{:.0} KB", size_kb)
                };
                @let display_ts = chrono::DateTime::parse_from_rfc3339(&snapshot.timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%d %b %Y, %I:%M %p").to_string())
                    .unwrap_or_else(|_| snapshot.timestamp.clone());
                option value=(snapshot.key) {
                    (format!("{} — {}", display_ts, size_str))
                }
            }
        }
    })
}

/// Authenticated snapshot trigger POST handler.
/// Creates a snapshot immediately and redirects back to settings.
pub async fn settings_backup_snapshot(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    match backup::create_snapshot(&state.db().await, &state.db_path, &config).await {
        Ok(key) => {
            tracing::info!("Snapshot created: {key}");
            Ok(Redirect::to("/settings?flash=snapshot_created").into_response())
        }
        Err(e) => {
            tracing::error!("Snapshot failed: {e:?}");
            Ok(Redirect::to("/settings?flash=snapshot_failed").into_response())
        }
    }
}

// ── Public backup handlers (no auth required) ──

/// Public backup configuration POST handler.
pub async fn backup_configure(
    State(state): State<AppState>,
    _user: LoggedInUser,
    Form(form): Form<PublicBackupForm>,
) -> Result<axum::response::Response, AppError> {
    let endpoint = form.endpoint.filter(|s| !s.trim().is_empty());
    let access_key_id = form.access_key_id.filter(|s| !s.trim().is_empty());
    let secret_access_key = form.secret_access_key.filter(|s| !s.trim().is_empty());
    let b2_key_id = form.b2_key_id.filter(|s| !s.trim().is_empty());
    let b2_application_key = form.b2_application_key.filter(|s| !s.trim().is_empty());
    let b2_endpoint = form.b2_endpoint.filter(|s| !s.trim().is_empty());

    // Pick region based on provider
    let region = if form.provider == "b2" {
        form.b2_region
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "us-east-1".to_string())
    } else {
        form.region
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "us-east-1".to_string())
    };

    // Preserve existing secrets if not re-entered
    let secret_access_key = match secret_access_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(&state.db().await).await?;
            existing.and_then(|c| c.secret_access_key)
        }
    };
    let b2_application_key = match b2_application_key {
        Some(s) => Some(s),
        None => {
            let existing = backup::get_config(&state.db().await).await?;
            existing.and_then(|c| c.b2_application_key)
        }
    };

    let config = backup::BackupConfig {
        id: Uuid::nil(), // Will be set by save_config if new
        provider: form.provider,
        bucket: form.bucket,
        path: form.path,
        region,
        endpoint,
        access_key_id,
        secret_access_key,
        b2_key_id,
        b2_application_key,
        b2_endpoint,
        enabled: false,       // Start paused; user explicitly enables
        db_instance_id: None, // Will be assigned by save_config
        interval_minutes: form.interval_minutes.unwrap_or(60),
        max_snapshots: form.max_snapshots.unwrap_or(10),
    };

    // Preserve existing enabled state if updating
    let existing = backup::get_config(&state.db().await).await?;
    let config = match existing {
        Some(mut c) => {
            c.provider = config.provider;
            c.bucket = config.bucket;
            c.path = config.path;
            c.region = config.region;
            c.endpoint = config.endpoint;
            c.access_key_id = config.access_key_id;
            c.secret_access_key = config.secret_access_key;
            c.b2_key_id = config.b2_key_id;
            c.b2_application_key = config.b2_application_key;
            c.b2_endpoint = config.b2_endpoint;
            c
        }
        None => config,
    };

    backup::save_config(&state.db().await, &config).await?;

    // If the config is enabled, create a snapshot immediately
    if config.enabled {
        let pool = state.db().await.clone();
        if let Err(e) = backup::create_snapshot(&pool, &state.db_path, &config).await {
            tracing::warn!("Failed to create snapshot after config save: {e:?}");
        }
    }

    Ok(Redirect::to("/backup?flash=saved").into_response())
}

/// Public backup enable POST handler.
pub async fn backup_enable(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(&state.db().await, true).await?;
    // Create a snapshot now that backups are enabled
    if let Some(config) = backup::get_config(&state.db().await).await? {
        let pool = state.db().await.clone();
        if let Err(e) = backup::create_snapshot(&pool, &state.db_path, &config).await {
            tracing::warn!("Failed to create snapshot on enable: {e:?}");
        }
    }
    Ok(Redirect::to("/backup?flash=enabled").into_response())
}

/// Public backup disable POST handler.
pub async fn backup_disable(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    backup::set_enabled(&state.db().await, false).await?;
    Ok(Redirect::to("/backup?flash=disabled").into_response())
}

/// Public backup restore POST handler.
/// Downloads a snapshot from the remote bucket and swaps the database file.
pub async fn backup_restore(
    State(state): State<AppState>,
    Form(form): Form<RestoreForm>,
) -> Result<axum::response::Response, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(
                "No backup configuration found. Configure backup settings first.".into(),
            )
        })?;

    // The "timestamp" field now holds the full snapshot key
    let snapshot_key = form.timestamp.as_deref().unwrap_or("");

    match backup::restore_from_snapshot(&state.db, &state.db_path, &config, snapshot_key).await {
        Ok(()) => {
            tracing::info!("Restore complete, pool reconnected");
            // Re-seed admin user so the in-memory admin_user_id matches
            // the restored DB (which may have a different user ID).
            let pool = state.db().await;
            let current_hash = state.admin_password_hash.read().unwrap().clone();
            let (new_admin_id, _) = user::seed_admin(&pool, &state.admin_username, &current_hash)
                .await
                .map_err(|e| {
                    AppError::Internal(anyhow::anyhow!(
                        "failed to re-seed admin after restore: {e:?}"
                    ))
                })?;
            *state.admin_user_id.write().unwrap() = new_admin_id;
            drop(pool);
            tracing::info!("Admin user re-synced after restore (id={new_admin_id})");
            Ok(Redirect::to("/login?flash=restored").into_response())
        }
        Err(e) => {
            tracing::error!("Restore failed: {e:?}");
            Ok(Redirect::to("/backup?flash=restore_failed").into_response())
        }
    }
}

/// Public snapshot list HTMX endpoint.
pub async fn backup_restore_points(
    State(state): State<AppState>,
) -> Result<maud::Markup, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    let snapshots = backup::list_all_snapshots(&config)
        .await
        .unwrap_or_default();

    Ok(maud::html! {
        select name="timestamp" {
            @for snapshot in &snapshots {
                @let size_kb = snapshot.size as f64 / 1024.0;
                @let size_str = if size_kb >= 1024.0 {
                    format!("{:.1} MB", size_kb / 1024.0)
                } else {
                    format!("{:.0} KB", size_kb)
                };
                @let display_ts = chrono::DateTime::parse_from_rfc3339(&snapshot.timestamp)
                    .map(|dt| dt.with_timezone(&chrono::Local).format("%d %b %Y, %I:%M %p").to_string())
                    .unwrap_or_else(|_| snapshot.timestamp.clone());
                option value=(snapshot.key) {
                    (format!("{} — {}", display_ts, size_str))
                }
            }
        }
    })
}

/// Public snapshot trigger POST handler.
/// Creates a snapshot immediately and redirects back to the backup page.
pub async fn backup_snapshot(
    State(state): State<AppState>,
    _user: LoggedInUser,
) -> Result<axum::response::Response, AppError> {
    let config = backup::get_config(&state.db().await)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    match backup::create_snapshot(&state.db().await, &state.db_path, &config).await {
        Ok(key) => {
            tracing::info!("Snapshot created: {key}");
            Ok(Redirect::to("/backup?flash=snapshot_created").into_response())
        }
        Err(e) => {
            tracing::error!("Snapshot failed: {e:?}");
            Ok(Redirect::to("/backup?flash=snapshot_failed").into_response())
        }
    }
}
