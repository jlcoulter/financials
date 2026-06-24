use axum::body::Body;
use axum_extra::extract::cookie::Key;
use sqlx::{SqlitePool, sqlite::SqliteConnectOptions};
use std::str::FromStr;
use tower::ServiceExt;

/// Create an in-memory SQLite pool with migrations applied.
async fn test_db() -> SqlitePool {
    let options = SqliteConnectOptions::from_str("sqlite::memory:").unwrap();
    let pool = SqlitePool::connect_with(options).await.unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    pool
}

/// Create a test app state with a fresh in-memory DB.
fn test_state(db: SqlitePool) -> rust_web::AppState {
    rust_web::AppState {
        db,
        key: Key::generate(),
    }
}

// ── Auth / route tests ──

#[tokio::test]
async fn test_home_page() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_signup_page_loads() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/signup")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_login_page_loads() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
}

#[tokio::test]
async fn test_dashboard_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/dashboard")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_transactions_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_budgets_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/budgets")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_goals_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/goals")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_holidays_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/holidays")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_csv_export_requires_auth() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/transactions/export/csv")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_not_found() {
    let db = test_db().await;
    let state = test_state(db);
    let app = rust_web::app(state);

    let response = app
        .oneshot(
            axum::http::Request::builder()
                .uri("/this-does-not-exist")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}

// ── Model-level tests ──

#[tokio::test]
async fn test_create_user() {
    let db = test_db().await;
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, "testuser", &hash)
        .await
        .expect("create_user should succeed");

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE username = ?")
        .bind("testuser")
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_create_and_list_transactions() {
    let db = test_db().await;
    let user_id = "testuser_txn";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // Create transactions directly
    for i in 0..3 {
        let txn_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO transactions (txn_id, user_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, '2025-01-15', ?, 'Food', ?, 'expense')"
        )
        .bind(txn_id.to_string())
        .bind(user_id)
        .bind(-1000 * (i + 1))
        .bind(format!("Item {}", i))
        .execute(&db)
        .await
        .unwrap();
    }

    let count = rust_web::models::features::count_transactions(
        &db, user_id, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(count, 3);

    let txns = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(txns.len(), 3);
}

#[tokio::test]
async fn test_soft_delete_transaction() {
    let db = test_db().await;
    let user_id = "testuser_del";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    let txn_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO transactions (txn_id, user_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, '2025-01-15', -5000, 'Food', 'Groceries', 'expense')"
    )
    .bind(txn_id.to_string())
    .bind(user_id)
    .execute(&db)
    .await
    .unwrap();

    // Should appear in active queries
    let count_before = rust_web::models::features::count_transactions(
        &db, user_id, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(count_before, 1);

    // Soft delete
    rust_web::models::features::delete_transaction(&db, user_id, txn_id)
        .await
        .unwrap();

    // Should no longer appear
    let count_after = rust_web::models::features::count_transactions(
        &db, user_id, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(count_after, 0);

    // But still exists in DB
    let total = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM transactions WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(&db)
    .await
    .unwrap();
    assert_eq!(total, 1);
}

#[tokio::test]
async fn test_search_transactions() {
    let db = test_db().await;
    let user_id = "testuser_search";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    for (desc, amt) in [("Groceries at store", -5000), ("Rent payment", -150000)] {
        let txn_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO transactions (txn_id, user_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, '2025-01-15', ?, 'Food', ?, 'expense')"
        )
        .bind(txn_id.to_string())
        .bind(user_id)
        .bind(amt)
        .bind(desc)
        .execute(&db)
        .await
        .unwrap();
    }

    // Search for "groceries" (case-insensitive)
    let results = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, Some("groceries"), None, None,
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].description.contains("Groceries"));

    // Search for "rent"
    let results = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, Some("rent"), None, None,
    )
    .await
    .unwrap();
    assert_eq!(results.len(), 1);

    // No search = all results
    let all = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn test_pagination() {
    let db = test_db().await;
    let user_id = "testuser_page";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // Create 25 transactions
    for i in 0..25i64 {
        let txn_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO transactions (txn_id, user_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, '2025-01-15', ?, 'Food', ?, 'expense')"
        )
        .bind(txn_id.to_string())
        .bind(user_id)
        .bind(-1000 - i)
        .bind(format!("Item {}", i))
        .execute(&db)
        .await
        .unwrap();
    }

    let total = rust_web::models::features::count_transactions(
        &db, user_id, None, None, None, None, None,
    )
    .await
    .unwrap();
    assert_eq!(total, 25);

    // Page 1 (limit 20, offset 0)
    let page1 = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, None, Some(20), Some(0),
    )
    .await
    .unwrap();
    assert_eq!(page1.len(), 20);

    // Page 2 (limit 20, offset 20)
    let page2 = rust_web::models::features::list_transactions(
        &db, user_id, None, None, None, None, None, Some(20), Some(20),
    )
    .await
    .unwrap();
    assert_eq!(page2.len(), 5);
}

#[tokio::test]
async fn test_create_and_list_budgets() {
    let db = test_db().await;
    let user_id = "testuser_bud";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // create_or_update_budget(pool, user_id, category, month, planned_amount)
    rust_web::models::features::create_or_update_budget(
        &db, user_id, "Food", "2025-01", 50000,
    )
    .await
    .unwrap();

    let budgets = rust_web::models::features::list_budgets_for_month(&db, user_id, "2025-01")
        .await
        .unwrap();
    assert_eq!(budgets.len(), 1);
    assert_eq!(budgets[0].category, "Food");
    assert_eq!(budgets[0].planned_amount, 50000);
}

#[tokio::test]
async fn test_budget_upsert() {
    let db = test_db().await;
    let user_id = "testuser_upsert";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    rust_web::models::features::create_or_update_budget(
        &db, user_id, "Food", "2025-01", 50000,
    )
    .await
    .unwrap();

    // Upsert: same category + month = update
    rust_web::models::features::create_or_update_budget(
        &db, user_id, "Food", "2025-01", 75000,
    )
    .await
    .unwrap();

    let budgets = rust_web::models::features::list_budgets_for_month(&db, user_id, "2025-01")
        .await
        .unwrap();
    assert_eq!(budgets.len(), 1);
    assert_eq!(budgets[0].planned_amount, 75000);
}

#[tokio::test]
async fn test_create_and_list_goals() {
    let db = test_db().await;
    let user_id = "testuser_goals";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // create_savings_goal(pool, user_id, name, target_amount, current_amount, target_date, category)
    rust_web::models::features::create_savings_goal(
        &db, user_id, "Emergency Fund", 1000000, 0, Some("2025-12-31"), "Emergency",
    )
    .await
    .unwrap();

    let goals = rust_web::models::features::list_savings_goals(&db, user_id)
        .await
        .unwrap();
    assert_eq!(goals.len(), 1);
    assert_eq!(goals[0].name, "Emergency Fund");
}

#[tokio::test]
async fn test_create_and_list_holidays() {
    let db = test_db().await;
    let user_id = "testuser_hol";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // create_holiday(pool, user_id, name, start_date: NaiveDate, end_date: NaiveDate)
    let start = chrono::NaiveDate::parse_from_str("2025-12-24", "%Y-%m-%d").unwrap();
    let end = chrono::NaiveDate::parse_from_str("2025-12-26", "%Y-%m-%d").unwrap();
    rust_web::models::features::create_holiday(&db, user_id, "Christmas", start, end)
        .await
        .unwrap();

    let holidays = rust_web::models::features::list_holidays(&db, user_id)
        .await
        .unwrap();
    assert_eq!(holidays.len(), 1);
    assert_eq!(holidays[0].name, "Christmas");
}

#[tokio::test]
async fn test_sum_transactions() {
    let db = test_db().await;
    let user_id = "testuser_sum";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_id, &hash).await.unwrap();

    // Create income and expense transactions
    for (desc, amt) in [("Salary", 500000), ("Rent", -150000), ("Groceries", -50000), ("Bonus", 250000)] {
        let txn_id = uuid::Uuid::now_v7();
        sqlx::query(
            "INSERT INTO transactions (txn_id, user_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, '2025-01-15', ?, 'General', ?, 'expense')"
        )
        .bind(txn_id.to_string())
        .bind(user_id)
        .bind(amt)
        .bind(desc)
        .execute(&db)
        .await
        .unwrap();
    }

    let (income, expenses) = rust_web::models::features::sum_transactions(&db, user_id, None, None, None, None, None)
        .await
        .unwrap();
    assert_eq!(income, 750000);   // Salary + Bonus
    assert_eq!(expenses, 200000);  // Rent + Groceries (absolute)
}

#[tokio::test]
async fn test_budget_unique_per_user() {
    let db = test_db().await;
    let user_a = "user_a";
    let user_b = "user_b";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, user_a, &hash).await.unwrap();
    rust_web::models::user::create_user(&db, user_b, &hash).await.unwrap();

    // Both users can create a budget for the same category+month
    rust_web::models::features::create_or_update_budget(&db, user_a, "Food", "2025-01", 50000)
        .await.unwrap();
    rust_web::models::features::create_or_update_budget(&db, user_b, "Food", "2025-01", 75000)
        .await.unwrap();

    let budgets_a = rust_web::models::features::list_budgets_for_month(&db, user_a, "2025-01")
        .await.unwrap();
    let budgets_b = rust_web::models::features::list_budgets_for_month(&db, user_b, "2025-01")
        .await.unwrap();

    assert_eq!(budgets_a.len(), 1);
    assert_eq!(budgets_a[0].planned_amount, 50000);
    assert_eq!(budgets_b.len(), 1);
    assert_eq!(budgets_b[0].planned_amount, 75000);
}

#[tokio::test]
async fn test_update_goal_amount_requires_owner() {
    let db = test_db().await;
    let owner = "goal_owner";
    let other = "goal_other";
    let hash = bcrypt::hash("password123", bcrypt::DEFAULT_COST).unwrap();
    rust_web::models::user::create_user(&db, owner, &hash).await.unwrap();
    rust_web::models::user::create_user(&db, other, &hash).await.unwrap();

    rust_web::models::features::create_savings_goal(
        &db, owner, "Test Goal", 100000, 0, Some("2025-12-31"), "General",
    ).await.unwrap();

    let goals = rust_web::models::features::list_savings_goals(&db, owner).await.unwrap();
    assert_eq!(goals.len(), 1);
    let goal_id = goals[0].goal_id;

    // Other user cannot update this goal
    let result = rust_web::models::features::update_savings_goal_amount(&db, other, goal_id, 50000).await;
    assert!(result.is_err(), "Other user should not be able to update goal");
}