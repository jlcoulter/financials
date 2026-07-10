use rust_web::models::user;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::str::FromStr;
use uuid::Uuid;

async fn setup_db() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .create_if_missing(true);
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn create_user_and_get_by_username() {
    let pool = setup_db().await;
    let id = user::create_user(&pool, "alice", "hashed_pw_123")
        .await
        .unwrap();

    let result = user::get_user_by_username(&pool, "alice").await.unwrap();
    assert!(result.is_some());
    let (fetched_id, fetched_hash) = result.unwrap();
    assert_eq!(fetched_id, id);
    assert_eq!(fetched_hash, "hashed_pw_123");
}

#[tokio::test]
async fn create_user_duplicate_returns_error() {
    let pool = setup_db().await;
    user::create_user(&pool, "bob", "hash1").await.unwrap();

    let result = user::create_user(&pool, "bob", "hash2").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn get_user_by_username_not_found() {
    let pool = setup_db().await;
    let result = user::get_user_by_username(&pool, "nobody").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn get_username_by_id() {
    let pool = setup_db().await;
    let id = user::create_user(&pool, "charlie", "hash3").await.unwrap();

    let username = user::get_username_by_id(&pool, id).await.unwrap();
    assert_eq!(username, "charlie");
}

#[tokio::test]
async fn get_username_by_id_not_found() {
    let pool = setup_db().await;
    let fake_id = Uuid::now_v7();
    let result = user::get_username_by_id(&pool, fake_id).await;
    assert!(result.is_err());
}
