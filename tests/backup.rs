use rust_web::models::backup;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::str::FromStr;
use uuid::Uuid;

async fn setup_db() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true);
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool
}

async fn create_test_user(pool: &SqlitePool) -> Uuid {
    let id = Uuid::now_v7();
    let hash = bcrypt::hash("password", 4).unwrap();
    sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind("testuser")
        .bind(&hash)
        .execute(pool)
        .await
        .unwrap();
    id
}

#[tokio::test]
async fn save_and_get_config_s3() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;

    let config = backup::BackupConfig {
        id: Uuid::nil(),
        user_id,
        provider: "s3".to_string(),
        bucket: "my-bucket".to_string(),
        path: "backups".to_string(),
        region: "us-east-1".to_string(),
        endpoint: None,
        access_key_id: Some("AKIA123".to_string()),
        secret_access_key: Some("secret456".to_string()),
        b2_key_id: None,
        b2_application_key: None,
        b2_endpoint: None,
        enabled: false,
    };

    backup::save_config(&pool, user_id, &config).await.unwrap();
    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(fetched.provider, "s3");
    assert_eq!(fetched.bucket, "my-bucket");
    assert_eq!(fetched.region, "us-east-1");
    assert!(fetched.endpoint.is_none());
    assert_eq!(fetched.access_key_id.unwrap(), "AKIA123");
    assert!(!fetched.enabled);
}

#[tokio::test]
async fn save_and_get_config_b2() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;

    let config = backup::BackupConfig {
        id: Uuid::nil(),
        user_id,
        provider: "b2".to_string(),
        bucket: "b2-bucket".to_string(),
        path: "backups".to_string(),
        region: "us-west-1".to_string(),
        endpoint: None,
        access_key_id: None,
        secret_access_key: None,
        b2_key_id: Some("key-id-123".to_string()),
        b2_application_key: Some("app-key-456".to_string()),
        b2_endpoint: Some("s3.us-west-004.backblazeb2.com".to_string()),
        enabled: true,
    };

    backup::save_config(&pool, user_id, &config).await.unwrap();
    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(fetched.provider, "b2");
    assert_eq!(fetched.b2_key_id.unwrap(), "key-id-123");
    assert!(fetched.enabled);
}

#[tokio::test]
async fn get_config_returns_none_when_not_configured() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;
    let result = backup::get_config(&pool, user_id).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn save_config_upserts_existing() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;

    let config = backup::BackupConfig {
        id: Uuid::nil(),
        user_id,
        provider: "s3".to_string(),
        bucket: "original-bucket".to_string(),
        path: "backups".to_string(),
        region: "us-east-1".to_string(),
        endpoint: None,
        access_key_id: Some("key".to_string()),
        secret_access_key: Some("secret".to_string()),
        b2_key_id: None,
        b2_application_key: None,
        b2_endpoint: None,
        enabled: false,
    };

    backup::save_config(&pool, user_id, &config).await.unwrap();

    let mut updated = config;
    updated.bucket = "new-bucket".to_string();
    updated.enabled = true;
    backup::save_config(&pool, user_id, &updated).await.unwrap();

    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(fetched.bucket, "new-bucket");
    assert!(fetched.enabled);
}

#[tokio::test]
async fn set_enabled_toggles_state() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;

    let config = backup::BackupConfig {
        id: Uuid::nil(),
        user_id,
        provider: "s3".to_string(),
        bucket: "my-bucket".to_string(),
        path: "backups".to_string(),
        region: "us-east-1".to_string(),
        endpoint: None,
        access_key_id: Some("key".to_string()),
        secret_access_key: Some("secret".to_string()),
        b2_key_id: None,
        b2_application_key: None,
        b2_endpoint: None,
        enabled: false,
    };

    backup::save_config(&pool, user_id, &config).await.unwrap();
    backup::set_enabled(&pool, user_id, true).await.unwrap();

    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert!(fetched.enabled);

    backup::set_enabled(&pool, user_id, false).await.unwrap();
    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert!(!fetched.enabled);
}

#[tokio::test]
async fn set_enabled_without_config_returns_error() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;
    let result = backup::set_enabled(&pool, user_id, true).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn save_config_with_custom_endpoint() {
    let pool = setup_db().await;
    let user_id = create_test_user(&pool).await;

    let config = backup::BackupConfig {
        id: Uuid::nil(),
        user_id,
        provider: "s3".to_string(),
        bucket: "minio-bucket".to_string(),
        path: "backups".to_string(),
        region: "local".to_string(),
        endpoint: Some("https://minio.local:9000".to_string()),
        access_key_id: Some("miniokey".to_string()),
        secret_access_key: Some("miniosecret".to_string()),
        b2_key_id: None,
        b2_application_key: None,
        b2_endpoint: None,
        enabled: false,
    };

    backup::save_config(&pool, user_id, &config).await.unwrap();
    let fetched = backup::get_config(&pool, user_id).await.unwrap().unwrap();
    assert_eq!(fetched.endpoint.unwrap(), "https://minio.local:9000");
}
