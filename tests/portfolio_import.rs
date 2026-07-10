use rust_web::models::portfolio;
use std::collections::HashMap;

mod helpers {
    use sqlx::SqlitePool;
    use sqlx::sqlite::SqliteConnectOptions;
    use std::str::FromStr;
    use uuid::Uuid;

    /// Create an in-memory SQLite pool with migrations applied.
    pub async fn setup_db() -> SqlitePool {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePool::connect_with(options).await.unwrap();
        sqlx::migrate!().run(&pool).await.unwrap();
        pool
    }

    /// Create a user and portfolio, returning (user_id, portfolio_id).
    pub async fn setup_portfolio(pool: &SqlitePool) -> (Uuid, Uuid) {
        let user_id = Uuid::now_v7();
        let portfolio_id = Uuid::now_v7();
        sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
            .bind(user_id.to_string())
            .bind("testuser")
            .bind("nothash")
            .execute(pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO portfolios (portfolio_id, user_id, name) VALUES (?, ?, ?)")
            .bind(portfolio_id.to_string())
            .bind(user_id.to_string())
            .bind("Test Portfolio")
            .execute(pool)
            .await
            .unwrap();
        (user_id, portfolio_id)
    }
}

// ── Portfolio CSV import integration tests ──

#[tokio::test]
async fn import_csv_creates_new_items() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings,Mortgage\n01/07/2025,10000,150000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    columns.insert(
        2,
        portfolio::ColumnTarget::New {
            name: "Mortgage".to_string(),
            item_type: "debt".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.rows_imported, 1);
    assert_eq!(result.items_created, 2);

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 2);

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 2);
}

#[tokio::test]
async fn import_csv_flips_negative_debt_values() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Mortgage\n01/07/2025,-150000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Mortgage".to_string(),
            item_type: "debt".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();
    assert_eq!(result.rows_imported, 1);

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    let mortgage = items.iter().find(|i| i.name == "Mortgage").unwrap();
    assert_eq!(mortgage.item_type, "debt");

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    let mortgage_log = logs.iter().find(|l| l.item_id == mortgage.item_id).unwrap();
    assert_eq!(mortgage_log.balance_value, 15000000);
}

#[tokio::test]
async fn import_csv_keeps_positive_debt_values() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Mortgage\n01/07/2025,150000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Mortgage".to_string(),
            item_type: "debt".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    let mortgage = items.iter().find(|i| i.name == "Mortgage").unwrap();
    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    let mortgage_log = logs.iter().find(|l| l.item_id == mortgage.item_id).unwrap();
    assert_eq!(mortgage_log.balance_value, 15000000);
}

#[tokio::test]
async fn import_csv_does_not_flip_non_debt_negative() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Investment\n01/07/2025,-5000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Investment".to_string(),
            item_type: "investment".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    let inv = items.iter().find(|i| i.name == "Investment").unwrap();
    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    let inv_log = logs.iter().find(|l| l.item_id == inv.item_id).unwrap();
    assert!(inv_log.balance_value < 0);
}

#[tokio::test]
async fn import_csv_maps_to_existing_item() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let item_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    let csv = "Date,Savings\n01/07/2025,5000\n";
    let mut columns = HashMap::new();
    columns.insert(1, portfolio::ColumnTarget::Existing(item_id.to_string()));
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.rows_imported, 1);
    assert_eq!(result.items_created, 0);

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].balance_value, 500000);
    assert_eq!(logs[0].item_id, item_id);
}

#[tokio::test]
async fn import_csv_skips_columns() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings,Notes\n01/07/2025,5000,ignore me\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.rows_imported, 1);
    assert_eq!(result.items_created, 1);

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
}

#[tokio::test]
async fn import_csv_skips_empty_cells() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings,Checking\n01/07/2025,5000,\n02/07/2025,,3000\n03/07/2025,6000,4000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    columns.insert(
        2,
        portfolio::ColumnTarget::New {
            name: "Checking".to_string(),
            item_type: "cash".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.rows_imported, 3);
    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 4);
}

#[tokio::test]
async fn import_csv_upserts_existing_date() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv1 = "Date,Savings\n01/07/2025,5000\n";
    let csv2 = "Date,Savings\n01/07/2025,7000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    portfolio::import_csv(&pool, portfolio_id, csv1, &mapping)
        .await
        .unwrap();
    portfolio::import_csv(&pool, portfolio_id, csv2, &mapping)
        .await
        .unwrap();

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].balance_value, 700000);
}

#[tokio::test]
async fn import_csv_skips_invalid_dates() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings\nnot-a-date,5000\n01/07/2025,3000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.rows_imported, 1);
    assert_eq!(result.rows_skipped, 1);
}

#[tokio::test]
async fn import_csv_reuses_existing_item_by_name() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let existing_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    let csv = "Date,Savings\n01/07/2025,5000\n";
    let mut columns = HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns,
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping)
        .await
        .unwrap();

    assert_eq!(result.items_created, 0);
    assert_eq!(result.rows_imported, 1);

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, existing_id);
}

#[tokio::test]
async fn import_csv_empty_csv_returns_error() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings\n";
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns: HashMap::new(),
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping).await;
    assert!(result.is_err());
}

// ── Portfolio CRUD integration tests ──

#[tokio::test]
async fn portfolio_create_and_list() {
    let pool = helpers::setup_db().await;
    let (user_id, _) = helpers::setup_portfolio(&pool).await;

    let id = portfolio::create_portfolio(&pool, user_id, "Second Portfolio")
        .await
        .unwrap();

    let list = portfolio::list_portfolios(&pool, user_id).await.unwrap();
    assert_eq!(list.len(), 2);
    assert!(
        list.iter()
            .any(|(pid, name)| pid == &id && name == "Second Portfolio")
    );
}

#[tokio::test]
async fn portfolio_get_wrong_user_returns_error() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let other_user = uuid::Uuid::now_v7();
    sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
        .bind(other_user.to_string())
        .bind("otheruser")
        .bind("nothash")
        .execute(&pool)
        .await
        .unwrap();

    let result = portfolio::get_portfolio(&pool, portfolio_id, other_user).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn wealth_item_create_and_list() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, id);
    assert_eq!(items[0].name, "Savings");
    assert_eq!(items[0].item_type, "cash");
}

#[tokio::test]
async fn wealth_item_move_swaps_positions() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let id_a = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();
    let id_b = portfolio::create_wealth_item(&pool, portfolio_id, "Mortgage", "debt")
        .await
        .unwrap();

    // Initially: Savings(pos=0), Mortgage(pos=1)
    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items[0].item_id, id_a);
    assert_eq!(items[1].item_id, id_b);

    // Move Savings right (swap with Mortgage)
    portfolio::move_wealth_item(&pool, portfolio_id, id_a, "right")
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items[0].item_id, id_b);
    assert_eq!(items[1].item_id, id_a);
}

#[tokio::test]
async fn wealth_item_delete_soft_deletes() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    portfolio::delete_wealth_item(&pool, id).await.unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert!(items.is_empty());
}

#[tokio::test]
async fn wealth_item_rename() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    portfolio::rename_wealth_item(&pool, id, "Emergency Fund")
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items[0].name, "Emergency Fund");
}

#[tokio::test]
async fn wealth_item_change_type() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    portfolio::change_wealth_item_type(&pool, id, "investment")
        .await
        .unwrap();

    let items = portfolio::list_wealth_items(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(items[0].item_type, "investment");
}

#[tokio::test]
async fn balance_log_upsert_overwrites() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let item_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();
    let date = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();

    portfolio::upsert_balance_log(&pool, item_id, date, 500000)
        .await
        .unwrap();
    portfolio::upsert_balance_log(&pool, item_id, date, 700000)
        .await
        .unwrap();

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].balance_value, 700000);
}

#[tokio::test]
async fn rename_date_conflict_returns_error() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let item_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();
    let july = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    let august = chrono::NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();

    // Insert logs on both dates
    portfolio::upsert_balance_log(&pool, item_id, july, 500000)
        .await
        .unwrap();
    portfolio::upsert_balance_log(&pool, item_id, august, 700000)
        .await
        .unwrap();

    // Trying to rename Aug 1 → Jul 1 should conflict
    let result = portfolio::rename_date(&pool, portfolio_id, august, july).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn rename_date_succeeds_no_conflict() {
    let pool = helpers::setup_db().await;
    let (_, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let item_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();
    let july = chrono::NaiveDate::from_ymd_opt(2025, 7, 1).unwrap();
    let august = chrono::NaiveDate::from_ymd_opt(2025, 8, 1).unwrap();

    portfolio::upsert_balance_log(&pool, item_id, july, 500000)
        .await
        .unwrap();

    let count = portfolio::rename_date(&pool, portfolio_id, july, august)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
