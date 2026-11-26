use axum::{extract::State, routing::post, Json, Router};

use crate::{
    auth::AuthUser,
    constellation::{self, ConstellationGenerationReport, ConstellationRequest},
    error::AppError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/generate", post(generate))
}

/// 生成星座卫星对象（不自动写入场景，由调用方决定是否保存）。
async fn generate(
    _user: AuthUser,
    State(_state): State<AppState>,
    Json(request): Json<ConstellationRequest>,
) -> Result<Json<ConstellationGenerationReport>, AppError> {
    let report = constellation::generate(&request).map_err(AppError::BadRequest)?;
    Ok(Json(report))
}
