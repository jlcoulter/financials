use crate::error::AppError;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<Uuid, AppError> {
    let id = Uuid::now_v7();
    let result = sqlx::query(
        "INSERT INTO users (user_id, username, password_hash, password_change_required) \
         VALUES (?, ?, ?, 1)",
    )
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
/// If the user doesn't exist, creates it. If it does, returns the stored
/// password hash (never overwrites it).
/// Returns (user_id, actual_stored_password_hash).
pub async fn seed_admin(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<(Uuid, String), AppError> {
    // Check if admin user exists
    let existing = sqlx::query("SELECT user_id, password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    match existing {
        Some(row) => {
            let id_str: String = row.get("user_id");
            let user_id = Uuid::parse_str(&id_str).map_err(|e| AppError::Internal(e.into()))?;
            let stored_hash: String = row.get("password_hash");
            // User already exists — never overwrite the password on restart.
            // The user may have changed it via the settings page.
            Ok((user_id, stored_hash))
        }
        None => {
            let user_id = create_user(pool, username, password_hash).await?;
            Ok((user_id, password_hash.to_string()))
        }
    }
}

/// Check whether the given user still needs to change their password.
pub async fn password_change_required(pool: &SqlitePool, user_id: Uuid) -> Result<bool, AppError> {
    let row = sqlx::query("SELECT password_change_required FROM users WHERE user_id = ?")
        .bind(user_id.to_string())
        .fetch_optional(pool)
        .await?;

    match row {
        Some(r) => {
            let val: bool = r.get("password_change_required");
            Ok(val)
        }
        None => Ok(false),
    }
}

/// Update the user's password hash and clear the change-required flag.
pub async fn update_password(
    pool: &SqlitePool,
    user_id: Uuid,
    new_hash: &str,
) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE users SET password_hash = ?, password_change_required = 0 WHERE user_id = ?",
    )
    .bind(new_hash)
    .bind(user_id.to_string())
    .execute(pool)
    .await?;
    Ok(())
}
