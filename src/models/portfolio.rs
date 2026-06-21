use crate::error::AppError;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn create_portfolio(pool: &SqlitePool, name: &str) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let result = sqlx::query("INSERT INTO portfolios (portfolio_id, name) VALUES (?, ?)")
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
