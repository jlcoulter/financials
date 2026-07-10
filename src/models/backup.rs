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
    pub enabled: bool,
}

pub async fn get_config(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Option<BackupConfig>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, bool)>(
        "SELECT id, provider, bucket, path, region, endpoint, access_key_id, secret_access_key, b2_key_id, b2_application_key, enabled FROM backup_config WHERE user_id = ?",
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
             enabled = ?, updated_at = CURRENT_TIMESTAMP WHERE user_id = ?",
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
        .bind(config.enabled)
        .bind(user_id.to_string())
        .execute(pool)
        .await?;
    } else {
        let id = Uuid::now_v7();
        sqlx::query(
            "INSERT INTO backup_config (id, user_id, provider, bucket, path, region, endpoint, \
             access_key_id, secret_access_key, b2_key_id, b2_application_key, enabled) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
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
pub fn generate_litestream_yaml(db_path: &str, config: &BackupConfig) -> String {
    let mut yaml = String::new();
    yaml.push_str("dbs:\n");
    yaml.push_str(&format!("  - path: {}\n", db_path));
    yaml.push_str("    replicas:\n");
    yaml.push_str("      - url: ");

    match config.provider.as_str() {
        "b2" => {
            yaml.push_str(&format!("b2://{}\n", config.bucket));
            yaml.push_str("        auth:\n");
            yaml.push_str(&format!(
                "          account_id: {}\n",
                config.b2_key_id.as_deref().unwrap_or("")
            ));
            yaml.push_str(&format!(
                "          application_key: {}\n",
                config.b2_application_key.as_deref().unwrap_or("")
            ));
        }
        _ => {
            if let Some(endpoint) = &config.endpoint {
                yaml.push_str(&format!(
                    "s3://{}?endpoint={}&region={}\n",
                    config.bucket, endpoint, config.region
                ));
            } else {
                yaml.push_str(&format!(
                    "s3://{}?region={}\n",
                    config.bucket, config.region
                ));
            }
            yaml.push_str("        auth:\n");
            yaml.push_str(&format!(
                "          access_key_id: {}\n",
                config.access_key_id.as_deref().unwrap_or("")
            ));
            yaml.push_str(&format!(
                "          secret_access_key: {}\n",
                config.secret_access_key.as_deref().unwrap_or("")
            ));
        }
    }

    yaml.push_str(&format!("        path: {}\n", config.path));
    yaml
}

/// Synchronize litestream state with the database config.
/// - If an enabled config exists: writes the YAML config file and (re)starts litestream replicate.
/// - If no enabled config: stops litestream and removes the config file.
pub fn sync_litestream(pool: &SqlitePool, db_path: &str) -> Result<(), AppError> {
    // Find any enabled config — single-user app, so user_id doesn't matter
    let rt = tokio::runtime::Handle::current();
    let user_id: Option<Uuid> = rt.block_on(async {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT user_id FROM backup_config WHERE enabled = 1 LIMIT 1")
                .fetch_optional(pool)
                .await
                .map_err(|e| AppError::Internal(e.into()))?;
        Ok::<Option<Uuid>, AppError>(row.map(|(id,)| Uuid::parse_str(&id).unwrap_or_default()))
    })?;

    let config_path = "litestream.yml";

    match user_id {
        Some(uid) => {
            let config = rt
                .block_on(async { get_config(pool, uid).await })?
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Config disappeared")))?;

            // Write litestream.yml
            let yaml = generate_litestream_yaml(db_path, &config);
            tracing::info!("Writing litestream config to {}", config_path);
            tracing::debug!("Litestream config:\n{}", yaml);
            std::fs::write(config_path, &yaml).map_err(|e| AppError::Internal(e.into()))?;

            // Kill any existing litestream process
            stop_litestream();

            // Start litestream replicate
            tracing::info!("Starting litestream replicate with config {}", config_path);
            match std::process::Command::new("litestream")
                .arg("replicate")
                .arg("-config")
                .arg(config_path)
                .spawn()
            {
                Ok(child) => {
                    tracing::info!("Litestream process started (PID {})", child.id());
                }
                Err(e) => {
                    tracing::error!("Failed to start litestream: {e}");
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "Failed to start litestream: {e}. Is litestream installed and on PATH?"
                    )));
                }
            }
        }
        None => {
            tracing::info!("No enabled backup config — stopping litestream");
            stop_litestream();

            if Path::new(config_path).exists() {
                tracing::info!("Removing litestream config file");
                let _ = std::fs::remove_file(config_path);
            }
        }
    }

    Ok(())
}

/// Stop any running litestream process by sending SIGTERM.
fn stop_litestream() {
    // Find and kill litestream processes we spawned
    match std::process::Command::new("pkill")
        .args(["-f", "litestream replicate"])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                tracing::info!("Stopped litestream process");
            } else {
                tracing::debug!("No litestream process found to stop");
            }
        }
        Err(e) => {
            tracing::warn!("Failed to stop litestream: {e}");
        }
    }
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
            enabled: true,
        }
    }

    #[test]
    fn litestream_yaml_s3_basic() {
        let config = make_s3_config();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("path: /data/financials.db"));
        assert!(yaml.contains("s3://my-bucket?region=us-east-1"));
        assert!(yaml.contains("access_key_id: AKIA123"));
        assert!(yaml.contains("secret_access_key: secret456"));
        assert!(yaml.contains("path: db-backups"));
    }

    #[test]
    fn litestream_yaml_s3_custom_endpoint() {
        let mut config = make_s3_config();
        config.endpoint = Some("https://minio.example.com".to_string());
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(
            yaml.contains("s3://my-bucket?endpoint=https://minio.example.com&region=us-east-1")
        );
    }

    #[test]
    fn litestream_yaml_b2() {
        let config = make_b2_config();
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("b2://my-b2-bucket"));
        assert!(yaml.contains("account_id: b2-key-id"));
        assert!(yaml.contains("application_key: b2-app-key"));
        assert!(yaml.contains("path: db-backups"));
        assert!(!yaml.contains("s3://"));
    }

    #[test]
    fn litestream_yaml_missing_credentials_default_to_empty() {
        let mut config = make_s3_config();
        config.access_key_id = None;
        config.secret_access_key = None;
        let yaml = generate_litestream_yaml("/data/financials.db", &config);
        assert!(yaml.contains("access_key_id: "));
        assert!(yaml.contains("secret_access_key: "));
    }
}
