use crate::error::AppError;
use chrono::NaiveDate;
use sqlx::SqlitePool;
use uuid::Uuid;

// ── Transactions ──

#[derive(Debug, Clone)]
pub struct Transaction {
    pub txn_id: Uuid,
    pub txn_date: NaiveDate,
    pub amount: i64,
    pub category: String,
    pub description: String,
    pub txn_type: String,
}

pub async fn create_transaction(
    pool: &SqlitePool,
    user_id: &str,
    txn_date: NaiveDate,
    amount: i64,
    category: &str,
    description: &str,
    txn_type: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO transactions (txn_id, txn_date, amount, category, description, txn_type, user_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(txn_date.to_string())
    .bind(amount)
    .bind(category)
    .bind(description)
    .bind(txn_type)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn count_transactions(
    pool: &SqlitePool,
    user_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&str>,
    txn_type: Option<&str>,
    search: Option<&str>,
) -> Result<i64, AppError> {
    use sqlx::Row;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COUNT(*) as cnt FROM transactions WHERE user_id = "
    );
    qb.push_bind(user_id);
    qb.push(" AND deleted_at IS NULL");
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
    if let Some(s) = search {
        qb.push(" AND description LIKE ");
        qb.push_bind(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    }
    let row = qb.build().fetch_one(pool).await?;
    let cnt: i64 = row.get("cnt");
    Ok(cnt)
}

/// Sum income and expenses for filtered transactions (ignores pagination).
/// Returns (total_income, total_expenses) where expenses are positive numbers.
pub async fn sum_transactions(
    pool: &SqlitePool,
    user_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&str>,
    txn_type: Option<&str>,
    search: Option<&str>,
) -> Result<(i64, i64), AppError> {
    use sqlx::Row;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT COALESCE(SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), 0) as income, \
         COALESCE(SUM(CASE WHEN amount < 0 THEN -amount ELSE 0 END), 0) as expenses \
         FROM transactions WHERE user_id = "
    );
    qb.push_bind(user_id);
    qb.push(" AND deleted_at IS NULL");
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
    if let Some(s) = search {
        qb.push(" AND description LIKE ");
        qb.push_bind(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    }
    let row = qb.build().fetch_one(pool).await?;
    let income: i64 = row.get("income");
    let expenses: i64 = row.get("expenses");
    Ok((income, expenses))
}

/// Aggregate transactions by category, returning (category, income, expenses) tuples.
pub async fn sum_transactions_by_category(
    pool: &SqlitePool,
    user_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&str>,
    txn_type: Option<&str>,
    search: Option<&str>,
) -> Result<Vec<(String, i64, i64)>, AppError> {
    use sqlx::Row;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT category, \
         COALESCE(SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), 0) as income, \
         COALESCE(SUM(CASE WHEN amount < 0 THEN -amount ELSE 0 END), 0) as expenses \
         FROM transactions WHERE user_id = "
    );
    qb.push_bind(user_id);
    qb.push(" AND deleted_at IS NULL");
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
    if let Some(s) = search {
        qb.push(" AND description LIKE ");
        qb.push_bind(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    }
    qb.push(" GROUP BY category ORDER BY category");
    let rows = qb.build().fetch_all(pool).await?;
    let result = rows.into_iter().map(|row| {
        let cat: String = row.get("category");
        let inc: i64 = row.get("income");
        let exp: i64 = row.get("expenses");
        (cat, inc, exp)
    }).collect();
    Ok(result)
}

pub async fn list_transactions(
    pool: &SqlitePool,
    user_id: &str,
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<&str>,
    txn_type: Option<&str>,
    search: Option<&str>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<Transaction>, AppError> {
    use sqlx::Row;
    let mut qb = sqlx::QueryBuilder::new(
        "SELECT txn_id, txn_date, amount, category, description, txn_type FROM transactions WHERE user_id = "
    );
    qb.push_bind(user_id);
    qb.push(" AND deleted_at IS NULL");
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
    if let Some(s) = search {
        qb.push(" AND description LIKE ");
        qb.push_bind(format!("%{}%", s.replace('%', "\\%").replace('_', "\\_")));
    }
    qb.push(" ORDER BY txn_date DESC, created_at DESC");
    if let Some(lim) = limit {
        qb.push(" LIMIT ");
        qb.push_bind(lim);
    }
    if let Some(off) = offset {
        qb.push(" OFFSET ");
        qb.push_bind(off);
    }

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

pub async fn delete_transaction(pool: &SqlitePool, user_id: &str, txn_id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE transactions SET deleted_at = datetime('now') WHERE txn_id = ? AND user_id = ?")
        .bind(txn_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Transaction not found or not owned by you".into()));
    }
    Ok(())
}

pub async fn get_transaction(pool: &SqlitePool, user_id: &str, txn_id: Uuid) -> Result<Option<Transaction>, AppError> {
    let row = sqlx::query_as::<_, (String, String, i64, String, String, String)>(
        "SELECT txn_id, txn_date, amount, category, description, txn_type FROM transactions WHERE txn_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(txn_id.to_string())
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id, date, amount, category, description, txn_type)| Transaction {
        txn_id: Uuid::parse_str(&id).unwrap_or_default(),
        txn_date: NaiveDate::parse_from_str(&date, "%Y-%m-%d").unwrap_or_default(),
        amount,
        category,
        description,
        txn_type,
    }))
}

pub async fn update_transaction(
    pool: &SqlitePool,
    user_id: &str,
    txn_id: Uuid,
    txn_date: NaiveDate,
    amount: i64,
    category: &str,
    description: &str,
    txn_type: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE transactions SET txn_date = ?, amount = ?, category = ?, description = ?, txn_type = ? WHERE txn_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(txn_date.to_string())
        .bind(amount)
        .bind(category)
        .bind(description)
        .bind(txn_type)
        .bind(txn_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get monthly spending totals by category for a given month (YYYY-MM).
pub async fn monthly_spending_by_category(
    pool: &SqlitePool,
    user_id: &str,
    month: &str,
) -> Result<Vec<(String, i64)>, AppError> {
    let rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT category, SUM(ABS(amount)) FROM transactions WHERE user_id = ? AND deleted_at IS NULL AND txn_type='expense' AND substr(txn_date,1,7)=? GROUP BY category ORDER BY SUM(ABS(amount)) DESC",
    )
    .bind(user_id)
    .bind(month)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Get monthly totals (income and expense) for a range of months.
#[allow(dead_code)]
pub async fn monthly_totals(
    pool: &SqlitePool,
    user_id: &str,
    from: &str,
    to: &str,
) -> Result<Vec<(String, i64, i64)>, AppError> {
    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        "SELECT substr(txn_date,1,7), \
         SUM(CASE WHEN amount > 0 THEN amount ELSE 0 END), \
         SUM(CASE WHEN amount < 0 THEN ABS(amount) ELSE 0 END) \
         FROM transactions \
         WHERE user_id = ? AND deleted_at IS NULL AND txn_date >= ? AND txn_date <= ? \
         GROUP BY substr(txn_date,1,7) \
         ORDER BY substr(txn_date,1,7)",
    )
    .bind(user_id)
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
    pub month: String,
    pub planned_amount: i64,
}

pub async fn create_or_update_budget(
    pool: &SqlitePool,
    user_id: &str,
    category: &str,
    month: &str,
    planned_amount: i64,
) -> Result<(), AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO budgets (budget_id, category, month, planned_amount, user_id) VALUES (?, ?, ?, ?, ?) \
         ON CONFLICT(user_id, category, month) DO UPDATE SET planned_amount = excluded.planned_amount",
    )
    .bind(id.to_string())
    .bind(category)
    .bind(month)
    .bind(planned_amount)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_budgets_for_month(
    pool: &SqlitePool,
    user_id: &str,
    month: &str,
) -> Result<Vec<Budget>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT budget_id, category, month, planned_amount FROM budgets WHERE user_id = ? AND deleted_at IS NULL AND month = ? ORDER BY category",
    )
    .bind(user_id)
    .bind(month)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, cat, month, amt)| {
        let budget_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        Ok(Budget { budget_id, category: cat, month, planned_amount: amt })
    }).collect()
}

pub async fn get_budget(pool: &SqlitePool, user_id: &str, budget_id: Uuid) -> Result<Option<Budget>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, i64)>(
        "SELECT budget_id, category, month, planned_amount FROM budgets WHERE budget_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(budget_id.to_string())
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id_s, cat, month, amt)| {
        let budget_id = Uuid::parse_str(&id_s).unwrap();
        Budget { budget_id, category: cat, month, planned_amount: amt }
    }))
}

pub async fn delete_budget(pool: &SqlitePool, user_id: &str, budget_id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE budgets SET deleted_at = datetime('now') WHERE budget_id = ? AND user_id = ?")
        .bind(budget_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Budget not found or not owned by you".into()));
    }
    Ok(())
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
    user_id: &str,
    name: &str,
    target_amount: i64,
    current_amount: i64,
    target_date: Option<&str>,
    category: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO savings_goals (goal_id, name, target_amount, current_amount, target_date, category, user_id) VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(target_amount)
    .bind(current_amount)
    .bind(target_date)
    .bind(category)
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_savings_goals(pool: &SqlitePool, user_id: &str) -> Result<Vec<SavingsGoal>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, i64, i64, Option<String>, String)>(
        "SELECT goal_id, name, target_amount, current_amount, target_date, category FROM savings_goals WHERE user_id = ? AND deleted_at IS NULL ORDER BY created_at",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, name, target, current, td, cat)| {
        let goal_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        Ok(SavingsGoal { goal_id, name, target_amount: target, current_amount: current, target_date: td, category: cat })
    }).collect()
}

pub async fn update_savings_goal_amount(
    pool: &SqlitePool,
    user_id: &str,
    goal_id: Uuid,
    current_amount: i64,
) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE savings_goals SET current_amount = ?, updated_at = datetime('now') WHERE goal_id = ? AND user_id = ? AND deleted_at IS NULL")
        .bind(current_amount)
        .bind(goal_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Goal not found or not owned by you".into()));
    }
    Ok(())
}

pub async fn delete_savings_goal(pool: &SqlitePool, user_id: &str, goal_id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE savings_goals SET deleted_at = datetime('now') WHERE goal_id = ? AND user_id = ?")
        .bind(goal_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Goal not found or not owned by you".into()));
    }
    Ok(())
}

pub async fn get_savings_goal(pool: &SqlitePool, user_id: &str, goal_id: Uuid) -> Result<Option<SavingsGoal>, AppError> {
    let row = sqlx::query_as::<_, (String, String, i64, i64, Option<String>, String)>(
        "SELECT goal_id, name, target_amount, current_amount, target_date, category FROM savings_goals WHERE goal_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(goal_id.to_string())
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id, name, target, current, target_date, category)| SavingsGoal {
        goal_id: Uuid::parse_str(&id).unwrap_or_default(),
        name,
        target_amount: target,
        current_amount: current,
        target_date,
        category,
    }))
}

pub async fn update_savings_goal(
    pool: &SqlitePool,
    user_id: &str,
    goal_id: Uuid,
    name: &str,
    target_amount: i64,
    current_amount: i64,
    target_date: Option<&str>,
    category: &str,
) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE savings_goals SET name = ?, target_amount = ?, current_amount = ?, target_date = ?, category = ?, updated_at = datetime('now') WHERE goal_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(name)
        .bind(target_amount)
        .bind(current_amount)
        .bind(target_date)
        .bind(category)
        .bind(goal_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Goal not found or not owned by you".into()));
    }
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
    user_id: &str,
    name: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO holidays (holiday_id, name, start_date, end_date, user_id) VALUES (?, ?, ?, ?, ?)",
    )
    .bind(id.to_string())
    .bind(name)
    .bind(start_date.to_string())
    .bind(end_date.to_string())
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(id)
}

pub async fn list_holidays(pool: &SqlitePool, user_id: &str) -> Result<Vec<Holiday>, AppError> {
    let rows = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT holiday_id, name, start_date, end_date FROM holidays WHERE user_id = ? AND deleted_at IS NULL ORDER BY start_date",
    )
    .bind(user_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(|(id_s, name, sd, ed)| {
        let holiday_id = Uuid::parse_str(&id_s).map_err(|e| AppError::Internal(e.into()))?;
        let start_date = NaiveDate::parse_from_str(&sd, "%Y-%m-%d").map_err(|e| AppError::Internal(e.into()))?;
        let end_date = NaiveDate::parse_from_str(&ed, "%Y-%m-%d").map_err(|e| AppError::Internal(e.into()))?;
        Ok(Holiday { holiday_id, name, start_date, end_date })
    }).collect()
}

pub async fn delete_holiday(pool: &SqlitePool, user_id: &str, holiday_id: Uuid) -> Result<(), AppError> {
    let result = sqlx::query("UPDATE holidays SET deleted_at = datetime('now') WHERE holiday_id = ? AND user_id = ?")
        .bind(holiday_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Holiday not found or not owned by you".into()));
    }
    Ok(())
}

pub async fn get_holiday(pool: &SqlitePool, user_id: &str, holiday_id: Uuid) -> Result<Option<Holiday>, AppError> {
    let row = sqlx::query_as::<_, (String, String, String, String)>(
        "SELECT holiday_id, name, start_date, end_date FROM holidays WHERE holiday_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(holiday_id.to_string())
        .bind(user_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|(id_s, name, sd, ed)| {
        let holiday_id = Uuid::parse_str(&id_s).unwrap_or_default();
        let start_date = NaiveDate::parse_from_str(&sd, "%Y-%m-%d").unwrap_or_default();
        let end_date = NaiveDate::parse_from_str(&ed, "%Y-%m-%d").unwrap_or_default();
        Holiday { holiday_id, name, start_date, end_date }
    }))
}

pub async fn update_holiday(
    pool: &SqlitePool,
    user_id: &str,
    holiday_id: Uuid,
    name: &str,
    start_date: NaiveDate,
    end_date: NaiveDate,
) -> Result<(), AppError> {
    let result = sqlx::query(
        "UPDATE holidays SET name = ?, start_date = ?, end_date = ? WHERE holiday_id = ? AND user_id = ? AND deleted_at IS NULL"
    )
        .bind(name)
        .bind(start_date.to_string())
        .bind(end_date.to_string())
        .bind(holiday_id.to_string())
        .bind(user_id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::BadRequest("Holiday not found or not owned by you".into()));
    }
    Ok(())
}