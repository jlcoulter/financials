use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Portfolio ──

pub async fn create_portfolio(pool: &SqlitePool, name: &str) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO portfolios (portfolio_id, name) VALUES (?, ?)")
        .bind(id.to_string())
        .bind(name)
        .execute(pool)
        .await?;
    Ok(id)
}

pub async fn list_portfolios(pool: &SqlitePool) -> Result<Vec<(Uuid, String)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios ORDER BY created_at",
    )
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

pub async fn get_portfolio(pool: &SqlitePool, id: Uuid) -> Result<(Uuid, String), AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios WHERE portfolio_id = ?",
    )
    .bind(id.to_string())
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

pub async fn list_wealth_items(
    pool: &SqlitePool,
    portfolio_id: Uuid,
) -> Result<Vec<WealthItem>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT item_id, name, item_type FROM wealth_items WHERE portfolio_id = ? ORDER BY created_at",
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
         WHERE wi.portfolio_id = ? \
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
            Ok(BalanceLog {
                log_id,
                item_id,
                log_date,
                balance_value,
            })
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

pub async fn delete_portfolio(pool: &SqlitePool, portfolio_id: Uuid) -> Result<(), AppError> {
    // CASCADE will delete wealth_items and their balance_logs
    sqlx::query("DELETE FROM portfolios WHERE portfolio_id = ?")
        .bind(portfolio_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_wealth_item(pool: &SqlitePool, item_id: Uuid) -> Result<(), AppError> {
    // CASCADE will delete associated balance_logs
    sqlx::query("DELETE FROM wealth_items WHERE item_id = ?")
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
         (SELECT item_id FROM wealth_items WHERE portfolio_id = ?)",
    )
    .bind(log_date.to_string())
    .bind(portfolio_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}