use axum::{routing::get, Json, Router};
use serde::Serialize;

use crate::{auth, rate_limit, state::AppState};

mod ai;
mod compute;
mod constellations;
mod jobs;
mod scenarios;
mod spacetrack;

/// 挂载在 /api/v1 下的全部路由。
pub fn router(state: AppState) -> Router<AppState> {
    let limited = Router::new()
        .nest("/compute", compute::router())
        .nest("/scenarios", scenarios::router())
        .nest("/jobs", jobs::router())
        .nest("/constellations", constellations::router())
        .layer(axum::middleware::from_fn_with_state(state, rate_limit::limit));

    Router::new()
        .route("/health", get(health))
        .nest("/auth", auth::router())
        .nest("/ai", ai::router())
        .nest("/spacetrack", spacetrack::router())
        .merge(limited)
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
