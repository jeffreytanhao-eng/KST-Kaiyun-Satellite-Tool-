use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};

use crate::{
    auth::AuthUser,
    error::AppError,
    space::{OrbitElements, SatelliteState},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/state", post(compute_state))
}

#[derive(Deserialize)]
struct StateRequest {
    /// RFC3339 UTC 时刻，例如 2026-11-25T12:00:00Z。
    epoch: String,
    orbit: OrbitElements,
    /// 轨道采样点数，0 表示不返回轨道轨迹。
    #[serde(default)]
    samples: u32,
}

#[derive(Serialize)]
struct VectorView {
    position_km: [f64; 3],
    velocity_km_s: [f64; 3],
}

#[derive(Serialize)]
struct StateResponse {
    epoch: String,
    et_seconds: f64,
    eci: VectorView,
    ecef: VectorView,
    longitude_deg: f64,
    latitude_deg: f64,
    altitude_km: f64,
    /// [经度(度), 纬度(度), 高度(km)]，用于前端绘制轨道。
    orbit_track: Vec<[f64; 3]>,
}

async fn compute_state(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(payload): Json<StateRequest>,
) -> Result<Json<StateResponse>, AppError> {
    payload.orbit.validate()?;

    let et = state.space.epoch_to_et(&payload.epoch)?;
    let snapshot = state.space.state_at(&payload.orbit, et)?;
    let orbit_track = if payload.samples > 0 {
        state
            .space
            .orbit_track(&payload.orbit, et, payload.samples)?
    } else {
        Vec::new()
    };

    Ok(Json(StateResponse {
        epoch: state.space.et_to_iso(et),
        et_seconds: et,
        eci: vector_view(&snapshot),
        ecef: VectorView {
            position_km: snapshot.ecef.position,
            velocity_km_s: snapshot.ecef.velocity,
        },
        longitude_deg: snapshot.longitude_deg,
        latitude_deg: snapshot.latitude_deg,
        altitude_km: snapshot.altitude_km,
        orbit_track,
    }))
}

fn vector_view(snapshot: &SatelliteState) -> VectorView {
    VectorView {
        position_km: snapshot.eci.position,
        velocity_km_s: snapshot.eci.velocity,
    }
}
