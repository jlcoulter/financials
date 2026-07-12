use crate::error::AppError;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let result =
        sqlx::query("INSERT INTO users (user_id, username, password_hash) VALUES (?, ?, ?)")
            .bind(id.to_string())
            .bind(username)
            .bind(password_hash)
            .execute(pool)
            .await;

    match result {
        Ok(_) => Ok(id),
        Err(sqlx::Error::Database(ref db_err))
            if db_err.code().is_some_and(|c| c == SQLITE_CONSTRAINT_UNIQUE) =>
        {
            Err(AppError::DuplicateUser)
        }
        Err(e) => Err(AppError::Internal(e.into())),
    }
}

pub async fn get_user_by_username(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<(Uuid, String)>, AppError> {
    let row = sqlx::query("SELECT user_id, password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    let user_data = match row {
        Some(r) => {
            let id_str: String = r.get("user_id");
            let user_id = Uuid::parse_str(&id_str).map_err(|e| AppError::Internal(e.into()))?;
            let password_hash: String = r.get("password_hash");
            Some((user_id, password_hash))
        }
        None => None,
    };

    Ok(user_data)
}

const SQLITE_CONSTRAINT_UNIQUE: &str = "2067";

pub async fn get_username_by_id(pool: &SqlitePool, user_id: Uuid) -> Result<String, AppError> {
    let row = sqlx::query("SELECT username FROM users WHERE user_id = ?")
        .bind(user_id.to_string())
        .fetch_one(pool)
        .await?;
    let username: String = row.get("username");
    Ok(username)
}

/// Seed the admin user at startup.
/// If the user doesn't exist, creates it. If it does, updates the password hash.
/// Returns the user_id.
pub async fn seed_admin(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<Uuid, AppError> {
    // Check if admin user exists
    let existing = sqlx::query("SELECT user_id FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    match existing {
        Some(row) => {
            let id_str: String = row.get("user_id");
            let user_id = Uuid::parse_str(&id_str).map_err(|e| AppError::Internal(e.into()))?;
            // Update password hash in case it changed
            sqlx::query("UPDATE users SET password_hash = ? WHERE user_id = ?")
                .bind(password_hash)
                .bind(id_str)
                .execute(pool)
                .await?;
            Ok(user_id)
        }
        None => {
            let user_id = create_user(pool, username, password_hash).await?;
            Ok(user_id)
        }
    }
}
