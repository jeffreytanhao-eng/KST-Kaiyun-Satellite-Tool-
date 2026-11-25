use axum::{routing::get, Json, Router};
use serde::Serialize;

use crate::{auth, state::AppState};

mod ai;
mod compute;
mod jobs;
mod scenarios;

/// 挂载在 /api/v1 下的全部路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .nest("/auth", auth::router())
        .nest("/compute", compute::router())
        .nest("/scenarios", scenarios::router())
        .nest("/jobs", jobs::router())
        .nest("/ai", ai::router())
}

#[derive(Serialize)]
struct HealthResponse {
    service: &'static str,
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        service: "kst-server",
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
