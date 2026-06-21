use crate::error::AppError;
use sqlx::SqlitePool;

pub async fn get_password_hash(
    pool: &SqlitePool,
    username: &str,
) -> Result<Option<String>, AppError> {
    let hash =
        sqlx::query_scalar::<_, String>("SELECT password_hash FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(pool)
            .await?;
    Ok(hash)
}

pub async fn create_user(
    pool: &SqlitePool,
    username: &str,
    password_hash: &str,
) -> Result<(), AppError> {
    let result = sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(username)
        .bind(password_hash)
        .execute(pool)
        .await;

    match result {
        Ok(_) => Ok(()),
        Err(sqlx::Error::Database(ref db_err))
            if db_err
                .code()
                .map_or(false, |c| c == SQLITE_CONSTRAINT_UNIQUE) =>
        {
            Err(AppError::DuplicateUser)
        }
        Err(e) => Err(AppError::Internal(e.into())),
    }
}

const SQLITE_CONSTRAINT_UNIQUE: &str = "2067";
