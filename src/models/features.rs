use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Transactions ──

#[derive(Debug, Clone)]
pub struct Transaction {
    pub txn_id: Uuid,
    pub txn_date: NaiveDate,
    pub amount: i64,         // positive = income, negative = expense (cents)
    pub category: String,
    pub description: String,
    pub txn_type: String,    // "income" or "expense"
}

pub async fn create_transaction(
    pool: &SqlitePool,
    txn_date: NaiveDate,
    amount: i64,
    category: &str,
    description: &str,
    txn_type: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO transactions (txn_id, txn_date, amount, category, description, txn_type) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(txn_date.to_string())
    .bind(amount)
    .bind(category)
    .bind(description)
    .bind(txn_type)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_transactions(
    pool: &SqlitePool,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&str>,
    txn_type: Option<&str>,
) -> Result<Vec<Transaction>, AppError> {
    use sqlx::Row;
    let mut qb = sqlx::QueryBuilder::new("SELECT txn_id, txn_date, amount, category, description, txn_type FROM transactions WHERE 1=1");
    if let Some(f) = from {
        qb.push(" AND txn_date >= ");
        qb.push_bind(f.to_string());
    }
    if let Some(t) = to {
        qb.push(" AND txn_date <= ");
        qb.push_bind(t.to_string());
    }
    if let Some(c) = category {
        qb.push(" AND category = ");
        qb.push_bind(c.to_string());
    }
    if let Some(t) = txn_type {
        qb.push(" AND txn_type = ");
        qb.push_bind(t.to_string());
    }
    qb.push(" ORDER BY txn_date DESC, created_at DESC");

    let rows = qb.build().fetch_all(pool).await?;
    rows.into_iter().map(|row| {
        let txn_id = Uuid::parse_str(&row.get::<String, _>("txn_id")).map_err(|e| AppError::Internal(e.into()))?;
        let txn_date = NaiveDate::parse_from_str(&row.get::<String, _>("txn_date"), "%Y-%m-%d").map_err(|e| AppError::Internal(e.into()))?;
        let amount: i64 = row.get("amount");
        let category: String = row.get("category");
        let description: String = row.get("description");
        let txn_type: String = row.get("txn_type");
        Ok(Transaction { txn_id, txn_date, amount, category, description, txn_type })
    }).collect()
}

pub async fn delete_transaction(pool: &SqlitePool, txn_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM transactions WHERE txn_id = ?")
        .bind(txn_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

/// Get monthly spending totals by category for a given month (YYYY-MM).
pub async fn monthly_spending_by_category(
    pool: &SqlitePool,
    month: &str,  // "YYYY-MM"
) -> Result<Vec<(String, i64)>, AppError> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT category, SUM(ABS(amount)) FROM transactions WHERE txn_type='expense' AND substr(txn_date,1,7)=? GROUP BY category ORDER BY SUM(ABS(amount)) DESC",
    )
    .bind(month)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Get monthly totals (income and expense) for a range of months.
#[allow(dead_code)]
pub async fn monthly_totals(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<Vec<(String, i64, i64)>, AppError> {
    // Returns (month, income, expense)
    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT substr(txn_date,1,7), \
         SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), \
         SUM(CASE WHEN amount < 0 THEN ABS(amount) ELSE 0 END) \
         FROM transactions \
         WHERE txn_date >= ? AND txn_date <= ? \
         GROUP BY substr(txn_date,1,7) \
         ORDER BY substr(txn_date,1,7)",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── Budgets ──

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Budget {
    pub budget_id: Uuid,
    pub category: String,
    pub month: String,  // "YYYY-MM"
    pub planned_amount: i64,
}

pub async fn create_or_update_budget(
    pool: &SqlitePool,
    category: &str,
    month: &str,
    planned_amount: i64,
) -> Result<(), AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO budgets (budget_id, category, month, planned_amount) VALUES (?, ?, ?, ?) \
         ON CONFLICT(category, month) DO UPDATE SET planned_amount = excluded.planned_amount",
    )
    .bind(id.to_string())
    .bind(category)
    .bind(month)
    .bind(planned_amount)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_budgets_for_month(
    pool: &SqlitePool,
    month: &str,
) -> Result<Vec<Budget>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT budget_id, category, month, planned_amount FROM budgets WHERE month = ? ORDER BY category",
    )
    .bind(month)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, cat, month, amt)| {
        let budget_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        Ok(Budget { budget_id, category: cat, month, planned_amount: amt })
    }).collect()
}

// ── Savings Goals ──

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SavingsGoal {
    pub goal_id: Uuid,
    pub name: String,
    pub target_amount: i64,
    pub current_amount: i64,
    pub target_date: Option<String>,
    pub category: String,
}

pub async fn create_savings_goal(
    pool: &SqlitePool,
    name: &str,
    target_amount: i64,
    current_amount: i64,
    target_date: Option<&str>,
    category: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO savings_goals (goal_id, name, target_amount, current_amount, target_date, category) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(target_amount)
    .bind(current_amount)
    .bind(target_date)
    .bind(category)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_savings_goals(pool: &SqlitePool) -> Result<Vec<SavingsGoal>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, i64, i64, Option<String>, String)>(
        "SELECT goal_id, name, target_amount, current_amount, target_date, category FROM savings_goals ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, name, target, current, td, cat)| {
        let goal_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        Ok(SavingsGoal { goal_id, name, target_amount: target, current_amount: current, target_date: td, category: cat })
    }).collect()
}

pub async fn update_savings_goal_amount(
    pool: &SqlitePool,
    goal_id: Uuid,
    current_amount: i64,
) -> Result<(), AppError> {
    sqlx::query("UPDATE savings_goals SET current_amount = ? WHERE goal_id = ?")
        .bind(current_amount)
        .bind(goal_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn delete_savings_goal(pool: &SqlitePool, goal_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM savings_goals WHERE goal_id = ?")
        .bind(goal_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}

// ── Holidays ──

#[derive(Debug, Clone)]
pub struct Holiday {
    pub holiday_id: Uuid,
    pub name: String,
    pub start_date: NaiveDate,
    pub end_date: NaiveDate,
}

pub async fn create_holiday(
    pool: &SqlitePool,
    name: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO holidays (holiday_id, name, start_date, end_date) VALUES (?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(start_date.to_string())
    .bind(end_date.to_string())
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_holidays(pool: &SqlitePool) -> Result<Vec<Holiday>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT holiday_id, name, start_date, end_date FROM holidays ORDER BY start_date",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, name, sd, ed)| {
        let holiday_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        let start_date = NaiveDate::parse_from_str(&sd, "%Y-%m-%d").map_err(|e| AppError::Internal(e.into()))?;
        let end_date = NaiveDate::parse_from_str(&ed, "%Y-%m-%d").map_err(|e| AppError::Internal(e.into()))?;
        Ok(Holiday { holiday_id, name, start_date, end_date })
    }).collect()
}

pub async fn delete_holiday(pool: &SqlitePool, holiday_id: Uuid) -> Result<(), AppError> {
    sqlx::query("DELETE FROM holidays WHERE holiday_id = ?")
        .bind(holiday_id.to_string())
        .execute(pool)
        .await?;
    Ok(())
}