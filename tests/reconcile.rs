use rust_web::models::csv_import::CsvRow;
use rust_web::models::reconcile;
use sqlx::SqlitePool;
use sqlx::sqlite::SqliteConnectOptions;
use std::collections::HashMap;
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

async fn setup_user(pool: &SqlitePool) -> Uuid {
    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
        .bind(user_id.to_string())
        .bind("testuser")
        .bind("nothash")
        .execute(pool)
        .await
        .unwrap();
    user_id
}

fn empty_metadata() -> HashMap<String, String> {
    HashMap::new()
}

#[tokio::test]
async fn create_and_list_sessions() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let id = reconcile::create_session(&pool, user_id, "Bank Reconciliation")
        .await
        .unwrap();

    let sessions = reconcile::list_sessions(&pool, user_id).await.unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].0, id);
    assert_eq!(sessions[0].1, "Bank Reconciliation");
}

#[tokio::test]
async fn get_session_by_id() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let id = reconcile::create_session(&pool, user_id, "Test Session")
        .await
        .unwrap();

    let (fetched_id, name) = reconcile::get_session(&pool, id, user_id).await.unwrap();
    assert_eq!(fetched_id, id);
    assert_eq!(name, "Test Session");
}

#[tokio::test]
async fn get_session_wrong_user_returns_error() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Owned Session")
        .await
        .unwrap();

    let other_user = Uuid::now_v7();
    sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
        .bind(other_user.to_string())
        .bind("otheruser")
        .bind("nothash")
        .execute(&pool)
        .await
        .unwrap();

    let result = reconcile::get_session(&pool, session_id, other_user).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn delete_session_soft_deletes() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let id = reconcile::create_session(&pool, user_id, "To Delete")
        .await
        .unwrap();

    reconcile::delete_session(&pool, id).await.unwrap();

    let sessions = reconcile::list_sessions(&pool, user_id).await.unwrap();
    assert!(sessions.is_empty());
}

#[tokio::test]
async fn add_and_list_outgoing() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let id = reconcile::add_outgoing(
        &pool,
        session_id,
        date,
        5000,
        "Coffee Shop",
        &empty_metadata(),
    )
    .await
    .unwrap();

    let txns = reconcile::list_outgoing(&pool, session_id).await.unwrap();
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].txn_id, id);
    assert_eq!(txns[0].amount, 5000);
    assert_eq!(txns[0].vendor, "Coffee Shop");
    assert!(!txns[0].matched);
}

#[tokio::test]
async fn bulk_add_outgoing_deduplicates() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let txns = vec![
        CsvRow {
            date,
            amount: 5000,
            vendor: "Coffee Shop".to_string(),
            metadata: HashMap::new(),
        },
        CsvRow {
            date,
            amount: 3000,
            vendor: "Tea House".to_string(),
            metadata: HashMap::new(),
        },
        CsvRow {
            date,
            amount: 5000,
            vendor: "Coffee Shop".to_string(),
            metadata: HashMap::new(),
        }, // duplicate
    ];

    let count = reconcile::bulk_add_outgoing(&pool, session_id, &txns)
        .await
        .unwrap();
    assert_eq!(count, 2); // duplicate not inserted

    let all = reconcile::list_outgoing(&pool, session_id).await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn add_and_list_reconciled() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let id = reconcile::add_reconciled(&pool, session_id, date, 5000, "Bank", &empty_metadata())
        .await
        .unwrap();

    let txns = reconcile::list_reconciled(&pool, session_id).await.unwrap();
    assert_eq!(txns.len(), 1);
    assert_eq!(txns[0].txn_id, id);
}

#[tokio::test]
async fn bulk_add_reconciled_deduplicates() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let txns = vec![
        CsvRow {
            date,
            amount: 5000,
            vendor: "Bank A".to_string(),
            metadata: HashMap::new(),
        },
        CsvRow {
            date,
            amount: 5000,
            vendor: "Bank A".to_string(),
            metadata: HashMap::new(),
        }, // duplicate
    ];

    let count = reconcile::bulk_add_reconciled(&pool, session_id, &txns)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn link_and_unlink_transactions() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let out_id =
        reconcile::add_outgoing(&pool, session_id, date, 5000, "Coffee", &empty_metadata())
            .await
            .unwrap();
    let rec_id =
        reconcile::add_reconciled(&pool, session_id, date, 5000, "Bank", &empty_metadata())
            .await
            .unwrap();

    // Link them
    reconcile::link_transactions(&pool, out_id, rec_id)
        .await
        .unwrap();

    // Verify both are marked matched
    let out_txns = reconcile::list_outgoing(&pool, session_id).await.unwrap();
    assert!(out_txns[0].matched);
    let rec_txns = reconcile::list_reconciled(&pool, session_id).await.unwrap();
    assert!(rec_txns[0].matched);

    // Verify match exists
    let matches = reconcile::list_matches(&pool, session_id).await.unwrap();
    assert_eq!(matches.len(), 1);
    let match_id = matches[0].match_id;

    // Unlink
    reconcile::unlink_transaction(&pool, match_id)
        .await
        .unwrap();

    // Both should be unmatched now
    let out_txns = reconcile::list_outgoing(&pool, session_id).await.unwrap();
    assert!(!out_txns[0].matched);
    let rec_txns = reconcile::list_reconciled(&pool, session_id).await.unwrap();
    assert!(!rec_txns[0].matched);

    // No matches left
    let matches = reconcile::list_matches(&pool, session_id).await.unwrap();
    assert!(matches.is_empty());
}

#[tokio::test]
async fn auto_match_exact_amount() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let out_id =
        reconcile::add_outgoing(&pool, session_id, date, 5000, "Coffee", &empty_metadata())
            .await
            .unwrap();
    let rec_id =
        reconcile::add_reconciled(&pool, session_id, date, 5000, "Bank", &empty_metadata())
            .await
            .unwrap();

    let proposals = reconcile::auto_match(&pool, session_id, &[]).await.unwrap();

    assert_eq!(proposals.len(), 1);
    assert_eq!(proposals[0].outgoing_id, out_id);
    assert_eq!(proposals[0].reconciled_ids, vec![rec_id]);
}

#[tokio::test]
async fn auto_match_no_match_different_amounts() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    reconcile::add_outgoing(&pool, session_id, date, 5000, "Coffee", &empty_metadata())
        .await
        .unwrap();
    reconcile::add_reconciled(&pool, session_id, date, 9999, "Bank", &empty_metadata())
        .await
        .unwrap();

    let proposals = reconcile::auto_match(&pool, session_id, &[]).await.unwrap();

    assert!(proposals.is_empty());
}

#[tokio::test]
async fn auto_match_skips_already_matched() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let out_id =
        reconcile::add_outgoing(&pool, session_id, date, 5000, "Coffee", &empty_metadata())
            .await
            .unwrap();
    let rec_id =
        reconcile::add_reconciled(&pool, session_id, date, 5000, "Bank", &empty_metadata())
            .await
            .unwrap();

    // Manually link them first
    reconcile::link_transactions(&pool, out_id, rec_id)
        .await
        .unwrap();

    // Auto-match should find nothing — both already matched
    let proposals = reconcile::auto_match(&pool, session_id, &[]).await.unwrap();
    assert!(proposals.is_empty());
}

#[tokio::test]
async fn delete_session_cascades_to_matches() {
    let pool = setup_db().await;
    let user_id = setup_user(&pool).await;

    let session_id = reconcile::create_session(&pool, user_id, "Test")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    let out_id =
        reconcile::add_outgoing(&pool, session_id, date, 5000, "Coffee", &empty_metadata())
            .await
            .unwrap();
    let rec_id =
        reconcile::add_reconciled(&pool, session_id, date, 5000, "Bank", &empty_metadata())
            .await
            .unwrap();

    reconcile::link_transactions(&pool, out_id, rec_id)
        .await
        .unwrap();

    // Delete the session — should soft-delete everything
    reconcile::delete_session(&pool, session_id).await.unwrap();

    let out_txns = reconcile::list_outgoing(&pool, session_id).await.unwrap();
    assert!(out_txns.is_empty());
    let rec_txns = reconcile::list_reconciled(&pool, session_id).await.unwrap();
    assert!(rec_txns.is_empty());
}
