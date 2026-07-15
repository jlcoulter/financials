use crate::error::AppError;
use futures::StreamExt;
use object_store::path::Path as ObjectPath;
use object_store::{ObjectStore, ObjectStoreExt};
use sqlx::SqlitePool;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct BackupConfig {
    pub id: Uuid,
    pub provider: String,
    pub bucket: String,
    pub path: String,
    pub region: String,
    pub endpoint: Option<String>,
    pub access_key_id: Option<String>,
    pub secret_access_key: Option<String>,
    pub b2_key_id: Option<String>,
    pub b2_application_key: Option<String>,
    pub b2_endpoint: Option<String>,
    pub enabled: bool,
    /// Unique per database instance. Appended to the backup path so that
    /// a fresh DB doesn't collide with snapshots from a previous instance.
    pub db_instance_id: Option<String>,
    /// How often to automatically create snapshots, in minutes.
    pub interval_minutes: i64,
    /// Maximum number of snapshots to keep in the bucket.
    /// Oldest snapshots are pruned after each upload.
    pub max_snapshots: i64,
}

/// Build an `object_store` S3 client from the stored backup config.
/// Works for both S3 and B2 (which uses an S3-compatible API).
fn build_object_store(config: &BackupConfig) -> Result<Arc<dyn ObjectStore>, AppError> {
    let (access_key, secret_key, endpoint) = match config.provider.as_str() {
        "b2" => {
            let key = config.b2_key_id.as_deref().unwrap_or("").to_string();
            let secret = config
                .b2_application_key
                .as_deref()
                .unwrap_or("")
                .to_string();
            let ep = config
                .b2_endpoint
                .as_deref()
                .unwrap_or("s3.us-west-004.backblazeb2.com");
            // Ensure endpoint has https://
            let endpoint_url = if ep.starts_with("http://") || ep.starts_with("https://") {
                ep.to_string()
            } else {
                format!("https://{ep}")
            };
            (key, secret, endpoint_url)
        }
        _ => {
            // S3 or other S3-compatible storage
            let key = config.access_key_id.as_deref().unwrap_or("").to_string();
            let secret = config
                .secret_access_key
                .as_deref()
                .unwrap_or("")
                .to_string();
            let endpoint_url = config.endpoint.as_deref().map(|ep| {
                if ep.starts_with("http://") || ep.starts_with("https://") {
                    ep.to_string()
                } else {
                    format!("https://{ep}")
                }
            });
            (key, secret, endpoint_url.unwrap_or_default())
        }
    };

    let mut builder = object_store::aws::AmazonS3Builder::new()
        .with_bucket_name(&config.bucket)
        .with_region(&config.region)
        .with_access_key_id(&access_key)
        .with_secret_access_key(&secret_key);

    if !endpoint.is_empty() {
        builder = builder.with_endpoint(endpoint);
    }

    // B2 and many S3-compatible stores need path-style and allow HTTP
    if config.provider == "b2" {
        builder = builder.with_allow_http(true);
    }
    // Custom endpoints (MinIO, etc.) also need allow_http for non-TLS
    if config
        .endpoint
        .as_ref()
        .is_some_and(|e| e.starts_with("http://"))
    {
        builder = builder.with_allow_http(true);
    }

    builder
        .build()
        .map(|store| Arc::new(store) as Arc<dyn ObjectStore>)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to build S3 client: {e}")))
}

/// Build the object store path prefix for this DB instance.
/// Format: {config.path}/{db_instance_id}/
/// This isolates each DB instance's snapshots from previous ones.
fn snapshot_prefix(config: &BackupConfig) -> String {
    match &config.db_instance_id {
        Some(instance_id) => {
            let base = config.path.trim_end_matches('/');
            if base.is_empty() {
                format!("{}/", instance_id)
            } else {
                format!("{}/{}/", base, instance_id)
            }
        }
        None => {
            let base = config.path.trim_end_matches('/');
            if base.is_empty() {
                "/".to_string()
            } else {
                format!("{}/", base)
            }
        }
    }
}

/// A snapshot listed from the remote bucket.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The object key in the bucket (e.g., "financials-backups/019f.../2026-07-12T18:30:00.db")
    pub key: String,
    /// ISO 8601 timestamp extracted from the filename.
    pub timestamp: String,
    /// File size in bytes.
    pub size: u64,
    /// The db_instance_id this snapshot belongs to (extracted from the path).
    pub instance_id: Option<String>,
}

/// List all available snapshots from the remote bucket, across all DB instances.
/// This is used for the restore page so you can recover even when the DB is fresh.
pub async fn list_all_snapshots(config: &BackupConfig) -> Result<Vec<Snapshot>, AppError> {
    let store = build_object_store(config)?;
    // List under the path prefix, not the instance-specific prefix,
    // so we see snapshots from all DB instances.
    let base = config.path.trim_end_matches('/');
    let prefix_path = if base.is_empty() {
        ObjectPath::from("/")
    } else {
        ObjectPath::from(base)
    };

    let mut result = store.list(Some(&prefix_path));
    let mut snapshots = Vec::new();
    while let Some(item) = result.next().await {
        let meta =
            item.map_err(|e| AppError::Internal(anyhow::anyhow!("error listing snapshots: {e}")))?;
        let key = meta.location.to_string();

        // Only include .db files (our snapshot format)
        if !key.ends_with(".db") {
            continue;
        }

        // Extract instance_id from path: {path}/{instance_id}/{timestamp}.db
        let instance_id = extract_instance_id(&key, base);

        // Extract timestamp from filename: 2026-07-12T18:30:00Z.db
        let filename = key.rsplit('/').next().unwrap_or(&key);
        let timestamp = filename.trim_end_matches(".db").to_string();

        snapshots.push(Snapshot {
            key,
            timestamp,
            size: meta.size,
            instance_id,
        });
    }

    // Sort by timestamp descending (newest first)
    snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(snapshots)
}

/// Extract the db_instance_id from an object key.
/// Key format: {path}/{instance_id}/{timestamp}.db or {timestamp}.db (no instance_id)
fn extract_instance_id(key: &str, base_path: &str) -> Option<String> {
    // Remove the base path prefix and the filename
    let stripped = if base_path.is_empty() {
        key
    } else {
        key.strip_prefix(&format!("{}/", base_path)).unwrap_or(key)
    };
    // Remaining: "{instance_id}/{timestamp}.db" or "{timestamp}.db"
    let parts: Vec<&str> = stripped.split('/').collect();
    if parts.len() >= 2 {
        // First part is the instance_id
        Some(parts[0].to_string())
    } else {
        None
    }
}

/// List snapshots for the current DB instance only.
/// Used for pruning (we only prune our own snapshots).
pub async fn list_snapshots(config: &BackupConfig) -> Result<Vec<Snapshot>, AppError> {
    let store = build_object_store(config)?;
    let prefix = snapshot_prefix(config);
    let prefix_path = ObjectPath::from(prefix.trim_end_matches('/'));

    let mut result = store.list(Some(&prefix_path));
    let mut snapshots = Vec::new();
    while let Some(item) = result.next().await {
        let meta =
            item.map_err(|e| AppError::Internal(anyhow::anyhow!("error listing snapshots: {e}")))?;
        let key = meta.location.to_string();

        // Only include .db files (our snapshot format)
        if !key.ends_with(".db") {
            continue;
        }

        // Extract timestamp from filename: {prefix}2026-07-12T18:30:00Z.db
        let filename = key.rsplit('/').next().unwrap_or(&key);
        let timestamp = filename.trim_end_matches(".db").to_string();

        snapshots.push(Snapshot {
            key,
            timestamp,
            size: meta.size,
            instance_id: config.db_instance_id.clone(),
        });
    }

    // Sort by timestamp descending (newest first)
    snapshots.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    Ok(snapshots)
}

/// Create a snapshot: `sqlite3 .backup` to a temp file, then upload to the bucket.
pub async fn create_snapshot(
    pool: &SqlitePool,
    db_path: &str,
    config: &BackupConfig,
) -> Result<String, AppError> {
    // Create a local backup using sqlite3 .backup command
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let snapshot_filename = format!("{timestamp}.db");
    let snapshot_dir = format!("{db_path}.snapshots");
    let snapshot_path = format!("{snapshot_dir}/{snapshot_filename}");

    // Create the snapshots directory if needed
    std::fs::create_dir_all(&snapshot_dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to create snapshots dir: {e}")))?;

    // Use SQL backup to create a consistent snapshot
    // This copies the entire database to a new file without locking
    let backup_path = snapshot_path.clone();
    sqlx::query("VACUUM INTO ?1")
        .bind(&backup_path)
        .execute(pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("sqlite VACUUM INTO failed: {e}")))?;

    // Upload to the remote bucket
    let store = build_object_store(config)?;
    let prefix = snapshot_prefix(config);
    let object_key = format!("{}{}", prefix, snapshot_filename);
    let object_path = ObjectPath::from(object_key.as_str());

    let data = std::fs::read(&snapshot_path)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read snapshot file: {e}")))?;

    store
        .put(&object_path, data.into())
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to upload snapshot: {e}")))?;

    // Clean up local snapshot file
    let _ = std::fs::remove_file(&snapshot_path);

    // Prune old snapshots beyond the retention limit
    let existing = list_snapshots(config).await.unwrap_or_default();
    prune_snapshots(config, &existing).await?;

    tracing::info!("Snapshot uploaded: {object_key}");
    Ok(object_key)
}

/// Prune old snapshots beyond the configured retention limit.
/// Keeps the newest `max_snapshots` and deletes the rest.
pub async fn prune_snapshots(
    config: &BackupConfig,
    existing_snapshots: &[Snapshot],
) -> Result<usize, AppError> {
    let max = config.max_snapshots.max(1) as usize;
    if existing_snapshots.len() <= max {
        return Ok(0);
    }

    let store = build_object_store(config)?;
    let to_delete = &existing_snapshots[max..];
    let mut deleted = 0;

    for snapshot in to_delete {
        let path = ObjectPath::from(snapshot.key.as_str());
        match store.delete(&path).await {
            Ok(()) => {
                tracing::info!("Pruned snapshot: {}", snapshot.key);
                deleted += 1;
            }
            Err(e) => {
                tracing::warn!("Failed to prune snapshot {}: {e}", snapshot.key);
            }
        }
    }

    Ok(deleted)
}

/// Restore the database from a remote snapshot.
///
/// Downloads the snapshot, swaps the database file, and reconnects the pool
/// in-place — no process restart needed.
pub async fn restore_from_snapshot(
    db: &Arc<RwLock<SqlitePool>>,
    db_path: &str,
    config: &BackupConfig,
    snapshot_key: &str,
) -> Result<(), AppError> {
    let store = build_object_store(config)?;
    let object_path = ObjectPath::from(snapshot_key);

    // Download the snapshot
    let result = store
        .get(&object_path)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to download snapshot: {e}")))?;

    let bytes = result
        .bytes()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to read snapshot data: {e}")))?;

    let db_path_buf = std::path::Path::new(db_path);
    let db_dir = db_path_buf
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."))
        .to_string_lossy()
        .to_string();
    let restore_path = format!("{db_dir}/data.db.restore");

    // Write downloaded data to temp file
    std::fs::write(&restore_path, &bytes)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("failed to write restore file: {e}")))?;

    tracing::info!("Snapshot downloaded, replacing database file");

    // Close all connections in the pool
    let mut pool_guard = db.write().await;
    pool_guard.close().await;

    // Remove WAL/SHM from old database
    let wal_path = format!("{db_path}-wal");
    let shm_path = format!("{db_path}-shm");
    let lstream_dir = format!("{db_path}-litestream");
    let _ = std::fs::remove_file(&wal_path);
    let _ = std::fs::remove_file(&shm_path);
    let _ = std::fs::remove_dir_all(&lstream_dir);

    // Swap the database file
    std::fs::rename(&restore_path, db_path).map_err(|e| {
        let _ = std::fs::remove_file(&restore_path);
        AppError::Internal(e.into())
    })?;

    tracing::info!("Database file swapped, reconnecting pool");

    // Create a new pool connected to the restored database
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false);
    let new_pool = SqlitePool::connect_with(options)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    // Replace the pool in the RwLock
    *pool_guard = new_pool;
    drop(pool_guard);

    tracing::info!("Database restored and pool reconnected");

    // If enabled, create a new snapshot of the restored DB
    // (this also verifies the restored DB is healthy)
    if config.enabled {
        let pool = db.read().await.clone();
        if let Err(e) = create_snapshot(&pool, db_path, config).await {
            tracing::warn!("Post-restore snapshot failed (non-fatal): {e:?}");
        }
    }

    Ok(())
}

pub async fn get_config(pool: &SqlitePool) -> Result<Option<BackupConfig>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, bool, Option<String>, i64, i64)>(
        "SELECT id, provider, bucket, path, region, endpoint, access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint, enabled, db_instance_id, interval_minutes, max_snapshots FROM backup_config LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    match row {
        Some((
            id_str,
            provider,
            bucket,
            path,
            region,
            endpoint,
            access_key_id,
            secret_access_key,
            b2_key_id,
            b2_application_key,
            b2_endpoint,
            enabled,
            db_instance_id,
            interval_minutes,
            max_snapshots,
        )) => {
            let id = Uuid::parse_str(&id_str)?;
            Ok(Some(BackupConfig {
                id,
                provider,
                bucket,
                path,
                region,
                endpoint,
                access_key_id,
                secret_access_key,
                b2_key_id,
                b2_application_key,
                b2_endpoint,
                enabled,
                db_instance_id,
                interval_minutes,
                max_snapshots,
            }))
        }
        None => Ok(None),
    }
}

pub async fn save_config(pool: &SqlitePool, config: &BackupConfig) -> Result<(), AppError> {
    let existing = sqlx::query("SELECT id FROM backup_config LIMIT 1")
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        // Updating existing config — preserve the db_instance_id from the
        // config struct (which came from the DB via get_config). If somehow
        // missing, generate a new one, but normally it should always be set.
        let db_instance_id = match &config.db_instance_id {
            Some(id) => id.clone(),
            None => Uuid::now_v7().to_string(),
        };

        sqlx::query(
            "UPDATE backup_config SET provider = ?, bucket = ?, path = ?, region = ?, endpoint = ?, \
             access_key_id = ?, secret_access_key = ?, b2_key_id = ?, b2_application_key = ?, \
             b2_endpoint = ?, enabled = ?, db_instance_id = ?, interval_minutes = ?, max_snapshots = ?, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&config.provider)
        .bind(&config.bucket)
        .bind(&config.path)
        .bind(&config.region)
        .bind(&config.endpoint)
        .bind(&config.access_key_id)
        .bind(&config.secret_access_key)
        .bind(&config.b2_key_id)
        .bind(&config.b2_application_key)
        .bind(&config.b2_endpoint)
        .bind(config.enabled)
        .bind(&db_instance_id)
        .bind(config.interval_minutes)
        .bind(config.max_snapshots)
        .execute(pool)
        .await?;
    } else {
        // New config — always generate a fresh db_instance_id
        let id = Uuid::now_v7();
        let db_instance_id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO backup_config (id, provider, bucket, path, region, endpoint, \
             access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint, enabled, db_instance_id, interval_minutes, max_snapshots) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(&config.provider)
        .bind(&config.bucket)
        .bind(&config.path)
        .bind(&config.region)
        .bind(&config.endpoint)
        .bind(&config.access_key_id)
        .bind(&config.secret_access_key)
        .bind(&config.b2_key_id)
        .bind(&config.b2_application_key)
        .bind(&config.b2_endpoint)
        .bind(config.enabled)
        .bind(&db_instance_id)
        .bind(config.interval_minutes)
        .bind(config.max_snapshots)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn set_enabled(pool: &SqlitePool, enabled: bool) -> Result<(), AppError> {
    let result =
        sqlx::query("UPDATE backup_config SET enabled = ?, updated_at = CURRENT_TIMESTAMP")
            .bind(enabled)
            .execute(pool)
            .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("No backup config found".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_s3_config() -> BackupConfig {
        BackupConfig {
            id: Uuid::nil(),
            provider: "s3".to_string(),
            bucket: "my-bucket".to_string(),
            path: "db-backups".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: Some("AKIA123".to_string()),
            secret_access_key: Some("secret456".to_string()),
            b2_key_id: None,
            b2_application_key: None,
            b2_endpoint: None,
            enabled: true,
            db_instance_id: Some("019f5564-a32d-7573-966b-b9bd9afe0fc5".to_string()),
            interval_minutes: 60,
            max_snapshots: 10,
        }
    }

    fn make_b2_config() -> BackupConfig {
        BackupConfig {
            id: Uuid::nil(),
            provider: "b2".to_string(),
            bucket: "my-b2-bucket".to_string(),
            path: "db-backups".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            b2_key_id: Some("b2-key-id".to_string()),
            b2_application_key: Some("b2-app-key".to_string()),
            b2_endpoint: None,
            enabled: true,
            db_instance_id: Some("019f5564-a32d-7573-966b-b9bd9afe0fc5".to_string()),
            interval_minutes: 60,
            max_snapshots: 10,
        }
    }

    #[test]
    fn snapshot_prefix_with_instance_id() {
        let config = make_s3_config();
        let prefix = snapshot_prefix(&config);
        assert_eq!(prefix, "db-backups/019f5564-a32d-7573-966b-b9bd9afe0fc5/");
    }

    #[test]
    fn snapshot_prefix_without_instance_id() {
        let mut config = make_s3_config();
        config.db_instance_id = None;
        let prefix = snapshot_prefix(&config);
        assert_eq!(prefix, "db-backups/");
    }

    #[test]
    fn snapshot_prefix_empty_path() {
        let mut config = make_s3_config();
        config.path = String::new();
        config.db_instance_id = Some("abc-123".to_string());
        let prefix = snapshot_prefix(&config);
        assert_eq!(prefix, "abc-123/");
    }

    #[test]
    fn snapshot_prefix_empty_path_no_instance_id() {
        let mut config = make_s3_config();
        config.path = String::new();
        config.db_instance_id = None;
        let prefix = snapshot_prefix(&config);
        assert_eq!(prefix, "/");
    }

    #[test]
    fn build_object_store_s3() {
        let config = make_s3_config();
        let result = build_object_store(&config);
        assert!(result.is_ok(), "Should build S3 client: {:?}", result.err());
    }

    #[test]
    fn build_object_store_b2() {
        let config = make_b2_config();
        let result = build_object_store(&config);
        assert!(result.is_ok(), "Should build B2 client: {:?}", result.err());
    }

    #[test]
    fn build_object_store_custom_endpoint() {
        let mut config = make_s3_config();
        config.endpoint = Some("https://minio.example.com".to_string());
        let result = build_object_store(&config);
        assert!(result.is_ok());
    }

    #[test]
    fn extract_instance_id_from_key() {
        // Standard path: financials-backups/{instance_id}/{timestamp}.db
        assert_eq!(
            extract_instance_id(
                "financials-backups/abc-123/2026-07-12T18:30:00Z.db",
                "financials-backups"
            ),
            Some("abc-123".to_string())
        );
        // No base path
        assert_eq!(
            extract_instance_id("abc-123/2026-07-12T18:30:00Z.db", ""),
            Some("abc-123".to_string())
        );
        // No instance ID (old format or flat path)
        assert_eq!(extract_instance_id("2026-07-12T18:30:00Z.db", ""), None);
        // Base path doesn't match — strip_prefix falls back to full key
        // so the first segment is "other-path"
        assert_eq!(
            extract_instance_id(
                "other-path/abc-123/2026-07-12T18:30:00Z.db",
                "financials-backups"
            ),
            Some("other-path".to_string())
        );
    }
}
