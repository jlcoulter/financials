use crate::error::AppError;
use sqlx::SqlitePool;
use std::path::Path;
use uuid::Uuid;

pub struct BackupConfig {
    pub id: Uuid,
    pub user_id: Uuid,
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
}

pub async fn get_config(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Option<BackupConfig>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, bool)>(
        "SELECT id, provider, bucket, path, region, endpoint, access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint, enabled FROM backup_config WHERE user_id = ?",
    )
    .bind(user_id.to_string())
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
        )) => {
            let id = Uuid::parse_str(&id_str)?;
            Ok(Some(BackupConfig {
                id,
                user_id,
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
            }))
        }
        None => Ok(None),
    }
}

pub async fn save_config(
    pool: &SqlitePool,
    user_id: Uuid,
    config: &BackupConfig,
) -> Result<(), AppError> {
    let existing = sqlx::query("SELECT id FROM backup_config WHERE user_id = ?")
        .bind(user_id.to_string())
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        sqlx::query(
            "UPDATE backup_config SET provider = ?, bucket = ?, path = ?, region = ?, endpoint = ?, \
             access_key_id = ?, secret_access_key = ?, b2_key_id = ?, b2_application_key = ?, \
             b2_endpoint = ?, enabled = ?, updated_at = CURRENT_TIMESTAMP WHERE user_id = ?",
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
        .bind(user_id.to_string())
        .execute(pool)
        .await?;
    } else {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO backup_config (id, user_id, provider, bucket, path, region, endpoint, \
             access_key_id, secret_access_key, b2_key_id, b2_application_key, b2_endpoint, enabled) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
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
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn set_enabled(pool: &SqlitePool, user_id: Uuid, enabled: bool) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE backup_config SET enabled = ?, updated_at = CURRENT_TIMESTAMP WHERE user_id = ?",
    )
    .bind(enabled)
    .bind(user_id.to_string())
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("No backup config found".into()));
    }
    Ok(())
}

/// Generate a litestream YAML config from the stored backup config.
///
/// Uses litestream's expanded replica format (type, bucket, path, etc.)
/// rather than URL format — the URL format doesn't reliably pass auth
/// credentials to the S3 client.
///
/// B2 uses the S3-compatible API — litestream does not support `b2://` URLs.
/// B2 configs use `type: s3` with the B2 S3 endpoint and B2 key ID /
/// application key as access-key-id / secret-access-key.
pub fn generate_litestream_yaml(db_path: &str, config: &BackupConfig) -> String {
    let mut yaml = String::new();
    yaml.push_str("dbs:\n");
    yaml.push_str(&format!("  - path: {}\n", db_path));
    yaml.push_str("    replicas:\n");

    match config.provider.as_str() {
        "b2" => {
            // B2 uses S3-compatible API in litestream
            let endpoint = config
                .b2_endpoint
                .as_deref()
                .unwrap_or("s3.us-west-004.backblazeb2.com");

            yaml.push_str("      - type: s3\n");
            yaml.push_str(&format!("        bucket: {}\n", config.bucket));
            if !config.path.is_empty() {
                yaml.push_str(&format!(
                    "        path: {}\n",
                    config.path.trim_end_matches('/')
                ));
            }
            // Ensure endpoint has https:// prefix
            if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                yaml.push_str(&format!("        endpoint: {}\n", endpoint));
            } else {
                yaml.push_str(&format!("        endpoint: https://{}\n", endpoint));
            }
            yaml.push_str(&format!("        region: {}\n", config.region));
            yaml.push_str(&format!(
                "        access-key-id: {}\n",
                config.b2_key_id.as_deref().unwrap_or("")
            ));
            yaml.push_str(&format!(
                "        secret-access-key: {}\n",
                config.b2_application_key.as_deref().unwrap_or("")
            ));
        }
        _ => {
            // S3 or other S3-compatible storage
            yaml.push_str("      - type: s3\n");
            yaml.push_str(&format!("        bucket: {}\n", config.bucket));
            if !config.path.is_empty() {
                yaml.push_str(&format!(
                    "        path: {}\n",
                    config.path.trim_end_matches('/')
                ));
            }
            if let Some(endpoint) = &config.endpoint {
                if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
                    yaml.push_str(&format!("        endpoint: {}\n", endpoint));
                } else {
                    yaml.push_str(&format!("        endpoint: https://{}\n", endpoint));
                }
            }
            yaml.push_str(&format!("        region: {}\n", config.region));
            yaml.push_str(&format!(
                "        access-key-id: {}\n",
                config.access_key_id.as_deref().unwrap_or("")
            ));
            yaml.push_str(&format!(
                "        secret-access-key: {}\n",
                config.secret_access_key.as_deref().unwrap_or("")
            ));
        }
    }

    yaml
}

/// Synchronize litestream config file with the database config.
/// - If an enabled config exists: writes litestream.yml and (re)starts litestream replicate.
/// - If no enabled config: stops litestream and removes litestream.yml.
pub async fn sync_litestream(
    pool: &SqlitePool,
    db_path: &str,
    config_dir: &str,
) -> Result<(), AppError> {
    // Find any enabled config — single-user app, so user_id doesn't matter
    let row: Option<(String,)> =
        sqlx::query_as("SELECT user_id FROM backup_config WHERE enabled = 1 LIMIT 1")
            .fetch_optional(pool)
            .await?;

    let config_path = format!("{config_dir}/litestream.yml");

    match row {
        Some((user_id_str,)) => {
            let uid = Uuid::parse_str(&user_id_str).map_err(|e| AppError::Internal(e.into()))?;
            let config = get_config(pool, uid)
                .await?
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Config disappeared")))?;

            // Write litestream.yml
            let yaml = generate_litestream_yaml(db_path, &config);
            tracing::info!("Writing litestream config to {}", config_path);
            tracing::debug!("Litestream config:\n{}", yaml);
            std::fs::write(&config_path, &yaml).map_err(|e| AppError::Internal(e.into()))?;

            // Try to start litestream replicate as a background process
            start_litestream(&config_path);
        }
        None => {
            tracing::info!("No enabled backup config — stopping litestream and removing config");
            stop_litestream();
            if Path::new(&config_path).exists() {
                tracing::info!("Removing {}", config_path);
                let _ = std::fs::remove_file(&config_path);
            }
        }
    }

    Ok(())
}

/// Start litestream replicate as a background process.
/// If litestream is not installed, logs a warning instead of failing.
fn start_litestream(config_path: &str) {
    // Kill any existing litestream process first
    stop_litestream();

    match tokio::process::Command::new("litestream")
        .arg("replicate")
        .arg("-config")
        .arg(config_path)
        .spawn()
    {
        Ok(child) => {
            tracing::info!("Litestream process started (PID {:?})", child.id());
        }
        Err(e) => {
            tracing::warn!(
                "Could not start litestream: {e}. \
                 Install litestream or use the Docker sidecar for automatic backups."
            );
        }
    }
}

/// Stop any running litestream replicate process.
fn stop_litestream() {
    match std::process::Command::new("pkill")
        .args(["-f", "litestream replicate"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                tracing::info!("Stopped existing litestream process");
            }
        }
        Err(_) => {
            tracing::debug!("pkill not available or no litestream process found");
        }
    }
}

/// Restore the database from a litestream backup.
///
/// This stops litestream, restores the latest snapshot to a temp file,
/// replaces the current database, and restarts litestream if it was enabled.
pub async fn restore_from_backup(
    pool: &SqlitePool,
    db_path: &str,
    config_dir: &str,
) -> Result<(), AppError> {
    // Find the backup config
    let row: Option<(String,)> = sqlx::query_as("SELECT user_id FROM backup_config LIMIT 1")
        .fetch_optional(pool)
        .await?;

    let user_id_str = row
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?
        .0;
    let uid = Uuid::parse_str(&user_id_str).map_err(|e| AppError::Internal(e.into()))?;
    let config = get_config(pool, uid)
        .await?
        .ok_or_else(|| AppError::BadRequest("No backup configuration found".into()))?;

    // Stop litestream before restoring
    tracing::info!("Stopping litestream before restore");
    stop_litestream();

    // Write a temporary litestream config for the restore
    let yaml = generate_litestream_yaml(db_path, &config);
    let config_path = format!("{config_dir}/litestream.yml");
    std::fs::write(&config_path, &yaml).map_err(|e| AppError::Internal(e.into()))?;

    // Restore to a temp file first, then swap
    let db_path_buf = Path::new(db_path);
    let db_dir = db_path_buf
        .parent()
        .unwrap_or(Path::new("."))
        .to_string_lossy()
        .to_string();
    let restore_path = format!("{db_dir}/data.db.restore");

    tracing::info!("Restoring database from backup to {restore_path}");

    let output = tokio::process::Command::new("litestream")
        .args([
            "restore",
            "-config",
            &config_path,
            "-o",
            &restore_path,
            db_path,
        ])
        .output()
        .await
        .map_err(|e| AppError::Internal(e.into()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(AppError::Internal(anyhow::anyhow!(
            "litestream restore failed: {stderr}"
        )));
    }

    // Close the current database connection pool so we can swap the file
    // We need to close all connections first
    tracing::info!("Restore downloaded successfully, replacing database file");

    // Replace the original database with the restored one
    std::fs::rename(&restore_path, db_path).map_err(|e| {
        // Clean up restore file on error
        let _ = std::fs::remove_file(&restore_path);
        AppError::Internal(e.into())
    })?;

    tracing::info!("Database restored successfully from backup");

    // Restart litestream if it was enabled
    if config.enabled {
        tracing::info!("Restarting litestream (was enabled)");
        start_litestream(&config_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_s3_config() -> BackupConfig {
        BackupConfig {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
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
        }
    }

    fn make_b2_config() -> BackupConfig {
        BackupConfig {
            id: Uuid::nil(),
            user_id: Uuid::nil(),
            provider: "b2".to_string(),
            bucket: "my-b2-bucket".to_string(),
            path: "db-backups".to_string(),
            region: "us-east-1".to_string(),
            endpoint: None,
            access_key_id: None,
            secret_access_key: None,
            b2_key_id: Some("b2-key-id".to_string()),
            b2_application_key: Some("b2-app-key".to_string()),
            b2_endpoint: None, // will default to s3.us-west-004.backblazeb2.com
            enabled: true,
        }
    }

    #[test]
    fn litestream_yaml_s3_basic() {
        let config = make_s3_config();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("path: /data/financials.db"));
        assert!(yaml.contains("type: s3"));
        assert!(yaml.contains("bucket: my-bucket"));
        assert!(yaml.contains("path: db-backups"));
        assert!(yaml.contains("region: us-east-1"));
        assert!(yaml.contains("access-key-id: AKIA123"));
        assert!(yaml.contains("secret-access-key: secret456"));
    }

    #[test]
    fn litestream_yaml_s3_no_path() {
        let mut config = make_s3_config();
        config.path = String::new();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("bucket: my-bucket"));
        assert!(!yaml.contains("        path:"));
    }

    #[test]
    fn litestream_yaml_s3_custom_endpoint() {
        let mut config = make_s3_config();
        config.endpoint = Some("https://minio.example.com".to_string());
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("endpoint: https://minio.example.com"));
        assert!(yaml.contains("region: us-east-1"));
    }

    #[test]
    fn litestream_yaml_b2_uses_s3_protocol() {
        let config = make_b2_config();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        // B2 uses S3-compatible expanded format
        assert!(yaml.contains("type: s3"));
        assert!(yaml.contains("bucket: my-b2-bucket"));
        assert!(yaml.contains("path: db-backups"));
        assert!(yaml.contains("endpoint: https://s3.us-west-004.backblazeb2.com"));
        assert!(yaml.contains("access-key-id: b2-key-id"));
        assert!(yaml.contains("secret-access-key: b2-app-key"));
        // Must NOT use b2:// URLs — litestream doesn't support them
        assert!(!yaml.contains("b2://"));
    }

    #[test]
    fn litestream_yaml_b2_custom_endpoint() {
        let mut config = make_b2_config();
        config.b2_endpoint = Some("s3.eu-central-003.backblazeb2.com".to_string());
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("endpoint: https://s3.eu-central-003.backblazeb2.com"));
    }

    #[test]
    fn litestream_yaml_b2_no_path() {
        let mut config = make_b2_config();
        config.path = String::new();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("bucket: my-b2-bucket"));
        // No path prefix should be emitted when path is empty
        assert!(!yaml.contains("        path:"));
    }

    #[test]
    fn litestream_yaml_missing_credentials_default_to_empty() {
        let mut config = make_s3_config();
        config.access_key_id = None;
        config.secret_access_key = None;
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("access-key-id: "));
        assert!(yaml.contains("secret-access-key: "));
    }
}
