use chrono::NaiveDate;
use rust_web::models::csv_import;
use rust_web::models::portfolio;
use rust_web::utils;

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
        let user_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
        let portfolio_id = uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext));
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

// ── Utils tests ──

#[test]
fn parse_dollars_positive() {
    assert_eq!(utils::parse_dollars("$1,234.56").unwrap(), 123456);
}

#[test]
fn parse_dollars_negative() {
    assert_eq!(utils::parse_dollars("-$1,234.56").unwrap(), -123456);
}

#[test]
fn parse_dollars_accounting_parens() {
    assert_eq!(utils::parse_dollars("($1,234.56)").unwrap(), -123456);
}

#[test]
fn parse_dollars_plain_number() {
    assert_eq!(utils::parse_dollars("10000").unwrap(), 1000000);
}

#[test]
fn parse_dollars_negative_plain() {
    assert_eq!(utils::parse_dollars("-150000").unwrap(), -15000000);
}

#[test]
fn parse_dollars_empty() {
    assert!(utils::parse_dollars("").is_err());
}

#[test]
fn format_cents_positive() {
    assert_eq!(utils::format_cents(123456), "$1,234.56");
}

#[test]
fn format_cents_negative() {
    assert_eq!(utils::format_cents(-123456), "-$1,234.56");
}

#[test]
fn format_cents_zero() {
    assert_eq!(utils::format_cents(0), "$0.00");
}

// ── CSV import analysis tests (pure functions) ──

#[test]
fn analyze_csv_detects_date_and_amount_columns() {
    let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,3.20\n";
    let analysis = csv_import::analyze_csv(csv).unwrap();
    assert_eq!(analysis.detected.date_col, 0);
    assert_eq!(analysis.detected.amount_col, 2);
    assert_eq!(analysis.headers.len(), 3);
    assert_eq!(analysis.total_rows, 2);
}

#[test]
fn analyze_csv_detects_vendor_column() {
    let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n";
    let analysis = csv_import::analyze_csv(csv).unwrap();
    assert_eq!(analysis.detected.vendor_col, Some(1));
}

#[test]
fn analyze_csv_rejects_empty_csv() {
    let csv = "";
    let result = csv_import::analyze_csv(csv);
    assert!(result.is_err());
}

#[test]
fn analyze_csv_defaults_to_dm_y_format() {
    // When no dates can be parsed, the fallback default is %d/%m/%Y
    let csv = "Date,Amount\nnotadate,100\nnotadate,200\n";
    let analysis = csv_import::analyze_csv(csv).unwrap();
    assert_eq!(analysis.detected.date_format, "%d/%m/%Y");
}

#[test]
fn analyze_csv_detects_dm_y_from_data() {
    // Unambiguous d/m/Y dates (day > 12) are detected correctly
    let csv = "Date,Amount\n25/07/2025,100\n";
    let analysis = csv_import::analyze_csv(csv).unwrap();
    assert_eq!(analysis.detected.date_format, "%d/%m/%Y");
}

#[test]
fn analyze_csv_detects_ymd_format() {
    let csv = "Date,Amount\n2025-07-01,100\n";
    let analysis = csv_import::analyze_csv(csv).unwrap();
    assert_eq!(analysis.detected.date_format, "%Y-%m-%d");
}

// ── CSV import with mapping (pure function) ──

#[test]
fn parse_csv_with_mapping_basic() {
    let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,3.20\n";
    let mapping = csv_import::ColumnMapping {
        date_col: 0,
        amount_col: 2,
        vendor_col: Some(1),
        date_format: "%d/%m/%Y".to_string(),
    };
    let rows = csv_import::parse_csv_with_mapping(csv, &mapping).unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, NaiveDate::from_ymd_opt(2025, 7, 1).unwrap());
    assert_eq!(rows[0].1, 450); // 4.50 in cents
    assert_eq!(rows[0].2, "Coffee");
}

#[test]
fn parse_csv_with_mapping_skips_empty_amount() {
    let csv = "Date,Description,Amount\n01/07/2025,Coffee,4.50\n02/07/2025,Tea,\n";
    let mapping = csv_import::ColumnMapping {
        date_col: 0,
        amount_col: 2,
        vendor_col: Some(1),
        date_format: "%d/%m/%Y".to_string(),
    };
    let rows = csv_import::parse_csv_with_mapping(csv, &mapping).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].2, "Coffee");
}

#[test]
fn parse_csv_with_mapping_negative_amounts() {
    let csv = "Date,Amount\n01/07/2025,-150000\n";
    let mapping = csv_import::ColumnMapping {
        date_col: 0,
        amount_col: 1,
        vendor_col: None,
        date_format: "%d/%m/%Y".to_string(),
    };
    let rows = csv_import::parse_csv_with_mapping(csv, &mapping).unwrap();
    assert_eq!(rows[0].1, -15000000); // -$150,000 in cents
}

// ── Portfolio CSV import integration tests ──

#[tokio::test]
async fn import_csv_creates_new_items() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings,Mortgage\n01/07/2025,10000,150000\n";
    let mut columns = std::collections::HashMap::new();
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

    // Mortgage is a debt; CSV has -150000 which should be stored as 150000 (positive)
    let csv = "Date,Mortgage\n01/07/2025,-150000\n";
    let mut columns = std::collections::HashMap::new();
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
    // Negative CSV value for debt should be stored as positive
    assert_eq!(mortgage_log.balance_value, 15000000); // 150,000.00 in cents, positive
}

#[tokio::test]
async fn import_csv_keeps_positive_debt_values() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    // Positive value for a debt item stays positive
    let csv = "Date,Mortgage\n01/07/2025,150000\n";
    let mut columns = std::collections::HashMap::new();
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
    assert_eq!(mortgage_log.balance_value, 15000000); // stored positive
}

#[tokio::test]
async fn import_csv_does_not_flip_non_debt_negative() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    // A negative value on a non-debt item (investment) should stay negative
    let csv = "Date,Investment\n01/07/2025,-5000\n";
    let mut columns = std::collections::HashMap::new();
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
    // Non-debt items keep their sign — negative stays negative
    assert!(inv_log.balance_value < 0);
}

#[tokio::test]
async fn import_csv_maps_to_existing_item() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    // Create an existing wealth item
    let item_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    let csv = "Date,Savings\n01/07/2025,5000\n";
    let mut columns = std::collections::HashMap::new();
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
    assert_eq!(result.items_created, 0); // No new items — mapped to existing

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    assert_eq!(logs.len(), 1);
    assert_eq!(logs[0].balance_value, 500000); // 5000.00 in cents
    assert_eq!(logs[0].item_id, item_id);
}

#[tokio::test]
async fn import_csv_skips_columns() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings,Notes\n01/07/2025,5000,ignore me\n";
    let mut columns = std::collections::HashMap::new();
    columns.insert(
        1,
        portfolio::ColumnTarget::New {
            name: "Savings".to_string(),
            item_type: "cash".to_string(),
        },
    );
    // Column 2 (Notes) is not in the mapping → skipped
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

    // Only one balance log — the Notes column was skipped
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
    let mut columns = std::collections::HashMap::new();
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
    // Row 1: 1 log (Checking empty), Row 2: 1 log (Savings empty), Row 3: 2 logs = 4 total
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
    let mut columns = std::collections::HashMap::new();
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
        columns: columns.clone(),
    };

    // Import once
    portfolio::import_csv(&pool, portfolio_id, csv1, &mapping)
        .await
        .unwrap();

    // Import again with updated value for same date
    portfolio::import_csv(&pool, portfolio_id, csv2, &mapping)
        .await
        .unwrap();

    let logs = portfolio::list_balance_logs(&pool, portfolio_id)
        .await
        .unwrap();
    // Should be 1 log (upserted), not 2
    assert_eq!(logs.len(), 1);
    // Value should be the updated one
    assert_eq!(logs[0].balance_value, 700000); // 7000.00 in cents
}

#[tokio::test]
async fn import_csv_skips_invalid_dates() {
    let pool = helpers::setup_db().await;
    let (_user_id, portfolio_id) = helpers::setup_portfolio(&pool).await;

    let csv = "Date,Savings\nnot-a-date,5000\n01/07/2025,3000\n";
    let mut columns = std::collections::HashMap::new();
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

    // Pre-create a Savings item
    let existing_id = portfolio::create_wealth_item(&pool, portfolio_id, "Savings", "cash")
        .await
        .unwrap();

    // Import with ColumnTarget::New for "Savings" — should reuse the existing item
    let csv = "Date,Savings\n01/07/2025,5000\n";
    let mut columns = std::collections::HashMap::new();
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

    assert_eq!(result.items_created, 0); // Reused existing, not created
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

    let csv = "Date,Savings\n"; // Header only, no data rows
    let mapping = portfolio::PortfolioColumnMapping {
        date_col: 0,
        date_format: "%d/%m/%Y".to_string(),
        columns: std::collections::HashMap::new(),
    };

    let result = portfolio::import_csv(&pool, portfolio_id, csv, &mapping).await;

    assert!(result.is_err());
}
