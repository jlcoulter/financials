use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::response::Response;
use crate::layout::error_box;

#[derive(Debug)]
pub enum AppError {
    Internal(anyhow::Error),
    BadRequest(String),
    Unauthorized(String),
    DuplicateUser,
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Internal(err.into())
    }
}

impl From<bcrypt::BcryptError> for AppError {
    fn from(err: bcrypt::BcryptError) -> Self {
        AppError::Internal(err.into())
    }
}

impl From<String> for AppError {
    fn from(msg: String) -> Self {
        AppError::BadRequest(msg)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::Internal(err) => {
                tracing::error!(%err, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
            }
            AppError::BadRequest(msg) => {
                (StatusCode::BAD_REQUEST, error_box(&msg)).into_response()
            }
            AppError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, error_box(&msg)).into_response()
            }
            AppError::DuplicateUser => {
                (StatusCode::CONFLICT, error_box("Username already taken")).into_response()
            }
        }
    }
}