use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

pub struct WealthItem {
    pub item_id: Uuid,
    pub name: String,
    pub item_type: String,
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
    let rows = sqlx::query_as::<_,(String, String, String)>(
        "SELECT item_id, name, item_type FROM wealth_items WHERE portfolio_id = ? AND deleted_at IS NULL ORDER BY created_at",
    )
        .bind(portfolio_id.to_string())
        .fetch_all(pool)
        .await?;

    rows.into_iter()
        .map(|(id_str, name, item_type)| {
            let item_id = Uuid::parse_str(&id_str)?;
            Ok(WealthItem {
                item_id,
                name,
                item_type,
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
