use std::fmt;

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Unauthorized(&'static str),
    Internal(String),
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AppError::BadRequest(msg) => write!(f, "bad request: {msg}"),
            AppError::Unauthorized(msg) => write!(f, "unauthorized: {msg}"),
            AppError::Internal(msg) => write!(f, "internal error: {msg}"),
        }
    }
}

impl std::error::Error for AppError {}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message) = match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, "bad_request", msg),
            AppError::Unauthorized(msg) => {
                (StatusCode::UNAUTHORIZED, "unauthorized", msg.to_string())
            }
            AppError::Internal(msg) => {
                tracing::error!(error = %msg, "服务内部错误");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "服务器内部错误".to_string(),
                )
            }
        };

        (
            status,
            Json(json!({ "error": { "code": code, "message": message } })),
        )
            .into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Internal(err.to_string())
    }
}
