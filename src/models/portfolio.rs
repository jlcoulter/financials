use crate::error::AppError;
use crate::utils;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Portfolio ──

pub async fn create_portfolio(pool: &SqlitePool, name: &str, user_id: &str) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO portfolios (portfolio_id, name, user_id) VALUES (?, ?, ?)")
        .bind(id.to_string())
        .bind(name)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn list_portfolios(pool: &SqlitePool, user_id: &str) -> Result<Vec<(Uuid, String)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios WHERE user_id = ? AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id_str, name)| {
            Uuid::parse_str(&id_str)
                .map(|id| (id, name))
                .map_err(|e| AppError::Internal(e.into()))
        })
        .collect()
}

pub async fn get_portfolio(pool: &SqlitePool, id: Uuid, user_id: &str) -> Result<(Uuid, String), AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios WHERE portfolio_id = ? AND user_id = ? AND deleted_at IS NULL",
    )
    .bind(id.to_string())
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("Portfolio not found".into()))?;

    let parsed = Uuid::parse_str(&row.0).map_err(|e| AppError::Internal(e.into()))?;
    Ok((parsed, row.1))
}

// ── Wealth Items ──

pub struct WealthItem {
    pub item_id: Uuid,
    pub name: String,
    pub item_type: String,
}

pub struct WealthItemWithPortfolio {
    pub item_id: Uuid,
    pub portfolio_id: Uuid,
    pub name: String,
    pub item_type: String,
}

pub async fn list_wealth_items(
    pool: &SqlitePool,
    portfolio_id: Uuid,
) -> Result<Vec<WealthItem>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT item_id, name, item_type FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(portfolio_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id_str, name, item_type)| {
            Uuid::parse_str(&id_str)
                .map(|item_id| WealthItem { item_id, name, item_type })
                .map_err(|e| AppError::Internal(e.into()))
        })
        .collect()
}

pub async fn create_wealth_item(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    name: &str,
    item_type: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO wealth_items (item_id, portfolio_id, name, item_type) VALUES (?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(portfolio_id.to_string())
    .bind(name)
    .bind(item_type)
    .execute(pool)
    .await?;
    Ok(id)
}

// ── Balance Logs ──

#[allow(dead_code)]
pub struct BalanceLog {
    pub log_id: Uuid,
    pub item_id: Uuid,
    pub log_date: NaiveDate,
    pub balance_value: i64,
}

pub async fn list_balance_logs(
    pool: &SqlitePool,
    portfolio_id: Uuid,
) -> Result<Vec<BalanceLog>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT bl.log_id, bl.item_id, bl.log_date, bl.balance_value \
         FROM balance_logs bl \
         JOIN wealth_items wi ON bl.item_id = wi.item_id \
         WHERE wi.portfolio_id = ? AND wi.deleted_at IS NULL \
         ORDER BY bl.log_date DESC, wi.created_at",
    )
    .bind(portfolio_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(log_id_str, item_id_str, date_str, balance_value)| {
            let log_id = Uuid::parse_str(&log_id_str).map_err(|e| AppError::Internal(e.into()))?;
            let item_id = Uuid::parse_str(&item_id_str).map_err(|e| AppError::Internal(e.into()))?;
            let log_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| AppError::Internal(e.into()))?;
            Ok(BalanceLog { log_id, item_id, log_date, balance_value })
        })
        .collect()
}

pub async fn insert_balance_log(
    pool: &SqlitePool,
    item_id: Uuid,
    log_date: NaiveDate,
    balance_value: i64,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO balance_logs (log_id, item_id, log_date, balance_value) VALUES (?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(item_id.to_string())
    .bind(log_date.to_string())
    .bind(balance_value)
    .execute(pool)
    .await?;
    Ok(id)
}

/// Upsert: update if the (item_id, log_date) pair exists, insert otherwise.
pub async fn upsert_balance_log(
    pool: &SqlitePool,
    item_id: Uuid,
    log_date: NaiveDate,
    balance_value: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO balance_logs (log_id, item_id, log_date, balance_value) VALUES (?, ?, ?, ?) \
         ON CONFLICT(item_id, log_date) DO UPDATE SET balance_value = excluded.balance_value",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(item_id.to_string())
    .bind(log_date.to_string())
    .bind(balance_value)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Deletes ──

pub async fn delete_portfolio(pool: &SqlitePool, portfolio_id: Uuid, user_id: &str) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE portfolios SET deleted_at = datetime('now') WHERE portfolio_id = ? AND user_id = ? AND deleted_at IS NULL")
        .bind(portfolio_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Portfolio not found or not owned by you".into()));
    }
    Ok(())
}

pub async fn delete_wealth_item(pool: &SqlitePool, item_id: Uuid) -> Result<(), AppError> {
    sqlx::query("UPDATE wealth_items SET deleted_at = datetime('now') WHERE item_id = ?")
        .bind(item_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete all balance logs for a given item on a given date (a "row" in the grid).
pub async fn delete_balance_row(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    log_date: NaiveDate,
) -> Result<(), AppError> {
    sqlx::query(
        "DELETE FROM balance_logs WHERE log_date = ? AND item_id IN \
         (SELECT item_id FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL)",
    )
    .bind(log_date.to_string())
    .bind(portfolio_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch all wealth items for a user across all their portfolios (batch version to avoid N+1).
pub async fn list_all_wealth_items_for_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<WealthItemWithPortfolio>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT wi.item_id, wi.portfolio_id, wi.name, wi.item_type \
         FROM wealth_items wi \
         JOIN portfolios p ON wi.portfolio_id = p.portfolio_id \
         WHERE p.user_id = ? AND p.deleted_at IS NULL AND wi.deleted_at IS NULL \
         ORDER BY wi.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id_str, pid_str, name, item_type)| {
            let item_id = Uuid::parse_str(&id_str).map_err(|e| AppError::Internal(e.into()))?;
            let portfolio_id = Uuid::parse_str(&pid_str).map_err(|e| AppError::Internal(e.into()))?;
            Ok(WealthItemWithPortfolio { item_id, portfolio_id, name, item_type })
        })
        .collect()
}

/// Import CSV data into a portfolio.
///
/// Expected CSV format: first column is a date (YYYY-MM-DD), subsequent columns are
/// wealth item names. The header row defines the column mapping.
/// `item_type` is the default type for newly created items (asset, debt, investment).
/// `column_types` maps column indices to item types, overriding `item_type`.
pub async fn import_csv(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    csv_data: &str,
    item_type: &str,
    column_types: &std::collections::HashMap<usize, String>,
) -> Result<ImportResult, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(csv_data.as_bytes());

    let headers = reader.headers()
        .map_err(|e| AppError::BadRequest(format!("Failed to read CSV headers: {}", e)))?
        .clone();

    if headers.is_empty() {
        return Err(AppError::BadRequest("CSV file is empty".into()));
    }

    // First column must be date, rest are item names
    let date_header = &headers[0];
    if date_header.trim().is_empty() {
        return Err(AppError::BadRequest("First column header must be a date column".into()));
    }

    let item_columns: Vec<(String, String)> = headers.iter().skip(1).enumerate().map(|(i, name)| {
        let resolved_type = column_types.get(&(i + 1))
            .cloned()
            .unwrap_or_else(|| item_type.to_string());
        (name.trim().to_string(), resolved_type)
    }).collect();

    // Validate item names are not empty
    for (name, _) in &item_columns {
        if name.is_empty() {
            return Err(AppError::BadRequest("Column headers cannot be empty".into()));
        }
    }

    // Create wealth items that don't exist yet
    let existing_items = list_wealth_items(pool, portfolio_id).await?;
    let existing_names: std::collections::HashMap<String, Uuid> = existing_items.iter()
        .map(|wi| (wi.name.clone(), wi.item_id))
        .collect();

    let mut item_ids: Vec<(Uuid, String)> = Vec::new();
    for (name, itype) in &item_columns {
        if let Some(&id) = existing_names.get(name) {
            item_ids.push((id, name.clone()));
        } else {
            let id = create_wealth_item(pool, portfolio_id, name, itype).await?;
            item_ids.push((id, name.clone()));
        }
    }

    let mut rows_imported = 0usize;
    let mut rows_skipped = 0usize;

    for result in reader.records() {
        let record = match result {
            Ok(r) => r,
            Err(_) => {
                rows_skipped += 1;
                continue;
            }
        };

        // Parse date from first column
        let date_str = record.get(0).unwrap_or("").trim();
        if date_str.is_empty() {
            rows_skipped += 1;
            continue;
        }

        let log_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => {
                // Try alternate formats
                match NaiveDate::parse_from_str(date_str, "%m/%d/%Y") {
                    Ok(d) => d,
                    Err(_) => {
                        rows_skipped += 1;
                        continue;
                    }
                }
            }
        };

        // Upsert each value column
        for (i, (item_id, _name)) in item_ids.iter().enumerate() {
            let value_str = record.get(i + 1).unwrap_or("").trim();
            if value_str.is_empty() {
                continue; // Skip empty cells (no value for this date/item combo)
            }
            let cents = match utils::parse_dollars(value_str) {
                Ok(c) => c,
                Err(_) => continue, // Skip invalid values
            };
            upsert_balance_log(pool, *item_id, log_date, cents).await?;
        }

        rows_imported += 1;
    }

    Ok(ImportResult {
        rows_imported,
        rows_skipped,
        items_created: item_columns.len(),
    })
}

pub struct ImportResult {
    pub rows_imported: usize,
    pub rows_skipped: usize,
    pub items_created: usize,
}

/// Fetch all balance logs for a user across all their portfolios (batch version to avoid N+1).
pub async fn list_all_balance_logs_for_user(
    pool: &SqlitePool,
    user_id: &str,
) -> Result<Vec<BalanceLog>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT bl.log_id, bl.item_id, bl.log_date, bl.balance_value \
         FROM balance_logs bl \
         JOIN wealth_items wi ON bl.item_id = wi.item_id \
         JOIN portfolios p ON wi.portfolio_id = p.portfolio_id \
         WHERE p.user_id = ? AND p.deleted_at IS NULL AND wi.deleted_at IS NULL \
         ORDER BY bl.log_date DESC, wi.created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(log_id_str, item_id_str, date_str, balance_value)| {
            let log_id = Uuid::parse_str(&log_id_str).map_err(|e| AppError::Internal(e.into()))?;
            let item_id = Uuid::parse_str(&item_id_str).map_err(|e| AppError::Internal(e.into()))?;
            let log_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")
                .map_err(|e| AppError::Internal(e.into()))?;
            Ok(BalanceLog { log_id, item_id, log_date, balance_value })
        })
        .collect()
}