use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct WealthItem {
    pub item_id: Uuid,
    pub name: String,
    pub item_type: String,
    pub position: i32,
}

pub struct BalanceLog {
    pub log_id: Uuid,
    pub item_id: Uuid,
    pub log_date: NaiveDate,
    pub balance_value: i64,
}

pub async fn list_wealth_items(
    pool: &SqlitePool,
    portfolio_id: Uuid,
) -> Result<Vec<WealthItem>, AppError> {
    let rows = sqlx::query_as::<_,(String, String, String, i32)>(
        "SELECT item_id, name, item_type, position FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL ORDER BY position, created_at",
    )
        .bind(portfolio_id.to_string())
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|(id_str, name, item_type, position)| {
            let item_id = Uuid::parse_str(&id_str)?;
            Ok(WealthItem {
                item_id,
                name,
                item_type,
                position,
            })
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
    let max_pos: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(position), -1) FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL",
    )
    .bind(portfolio_id.to_string())
    .fetch_one(pool)
    .await?;
    sqlx::query(
        "INSERT INTO wealth_items (item_id, portfolio_id, name, item_type, position) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(portfolio_id.to_string())
    .bind(name)
    .bind(item_type)
    .bind(max_pos + 1)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn move_wealth_item(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    item_id: Uuid,
    direction: &str,
) -> Result<(), AppError> {
    let items = list_wealth_items(pool, portfolio_id).await?;
    let idx = items
        .iter()
        .position(|i| i.item_id == item_id)
        .ok_or_else(|| AppError::BadRequest("Item not found".into()))?;

    let swap_idx = match direction {
        "left" if idx > 0 => idx - 1,
        "right" if idx < items.len() - 1 => idx + 1,
        _ => return Ok(()), // already at edge, no-op
    };

    let a = &items[idx];
    let b = &items[swap_idx];

    // Swap positions
    sqlx::query("UPDATE wealth_items SET position = ? WHERE item_id = ?")
        .bind(b.position)
        .bind(a.item_id.to_string())
        .execute(pool)
        .await?;
    sqlx::query("UPDATE wealth_items SET position = ? WHERE item_id = ?")
        .bind(a.position)
        .bind(b.item_id.to_string())
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn delete_wealth_item(pool: &SqlitePool, item_id: Uuid) -> Result<(), AppError> {
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
    sqlx::query("UPDATE wealth_items SET deleted_at = ? WHERE item_id = ?")
        .bind(now)
        .bind(item_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_balance_logs(
    pool: &SqlitePool,
    portfolio_id: Uuid,
) -> Result<Vec<BalanceLog>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT bl.log_id, bl.item_id, bl.log_date, bl.balance_value FROM balance_logs bl JOIN wealth_items wi on bl.item_id = wi.item_id WHERE wi.portfolio_id = ? AND wi.deleted_at IS NULL ORDER BY bl.log_date DESC, wi.created_at",
    ).bind(portfolio_id.to_string())
    .fetch_all(pool).await?;

    rows.into_iter()
        .map(|(log_id_str, item_id_str, date_str, balance_value)| {
            let log_id = Uuid::parse_str(&log_id_str)?;
            let item_id = Uuid::parse_str(&item_id_str)?;
            let log_date = NaiveDate::parse_from_str(&date_str, "%Y-%m-%d")?;
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

/// Upsert: update if (item_id, log_date) exists, insert otherwise.
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

pub async fn rename_wealth_item(
    pool: &SqlitePool,
    item_id: Uuid,
    name: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE wealth_items SET name = ? WHERE item_id = ?")
        .bind(name)
        .bind(item_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn change_wealth_item_type(
    pool: &SqlitePool,
    item_id: Uuid,
    item_type: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE wealth_items SET item_type = ? WHERE item_id = ?")
        .bind(item_type)
        .bind(item_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Rename a date for all balance logs in a portfolio.
/// Updates all logs on `old_date` to `new_date` for items belonging to this portfolio.
/// Returns the number of rows updated, or a BadRequest error if the new date would
/// conflict with existing logs.
pub async fn rename_date(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    old_date: NaiveDate,
    new_date: NaiveDate,
) -> Result<usize, AppError> {
    let result = sqlx::query(
        "UPDATE balance_logs SET log_date = ? \
         WHERE log_date = ? AND item_id IN (\
           SELECT item_id FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL\
         ) AND deleted_at IS NULL",
    )
    .bind(new_date.to_string())
    .bind(old_date.to_string())
    .bind(portfolio_id.to_string())
    .execute(pool)
    .await;

    match result {
        Ok(r) => Ok(r.rows_affected() as usize),
        Err(sqlx::Error::Database(ref db_err))
            if crate::error::is_unique_constraint(db_err.as_ref()) =>
        {
            Err(AppError::BadRequest(format!(
                "Date {} already has entries in this portfolio",
                new_date
            )))
        }
        Err(e) => Err(e.into()),
    }
}

pub async fn create_portfolio(
    pool: &SqlitePool,
    user_id: Uuid,
    name: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let result =
        sqlx::query("INSERT INTO portfolios (portfolio_id, user_id, name) VALUES (?, ?, ?)")
            .bind(id.to_string())
            .bind(user_id.to_string())
            .bind(name)
            .execute(pool)
            .await?;
    Ok(id)
}

pub async fn list_portfolios(
    pool: &SqlitePool,
    user_id: Uuid,
) -> Result<Vec<(Uuid, String)>, AppError> {
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios WHERE user_id = ? AND deleted_at is NULL ORDER BY created_at",
    )
    .bind(user_id.to_string())
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|(id_str, name)| {
            let id = Uuid::parse_str(&id_str)?;
            Ok((id, name))
        })
        .collect()
}

pub async fn get_portfolio(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    user_id: Uuid,
) -> Result<(Uuid, String), AppError> {
    let row = sqlx::query_as::<_, (String, String)>(
        "SELECT portfolio_id, name FROM portfolios WHERE portfolio_id = ? AND user_id = ? AND deleted_at IS NULL",
    )
        .bind(portfolio_id.to_string())
        .bind(user_id.to_string())
        .fetch_optional(pool)
        .await?
    .ok_or_else(|| AppError::BadRequest("Portfolio not found".into()))?;
    let id = Uuid::parse_str(&row.0)?;
    Ok((id, row.1))
}

pub async fn rename_portfolio(
    pool: &SqlitePool,
    portfolio_id: Uuid,
    name: &str,
) -> Result<(), AppError> {
    sqlx::query("UPDATE portfolios SET name = ? WHERE portfolio_id = ?")
        .bind(name)
        .bind(portfolio_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}
