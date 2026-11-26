use axum::{
    extract::State,
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    ai::PRESETS,
    auth::AuthUser,
    environment::EnvironmentReport,
    error::AppError,
    link::{InterferenceReport, LinkBudgetReport},
    orbit::{AccessReport, ConstellationAnalysisReport, CoverageReport},
    policy::{self, actions},
    radar::RadarReport,
    scenario::ScenarioData,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(config))
        .route("/interpret", post(interpret))
        .route("/recommend", post(recommend))
        .route("/report", post(report))
        .route("/polish", post(polish))
        .route(
            "/interpretations",
            get(list_interpretations).post(save_interpretation),
        )
}

#[derive(Debug, Serialize)]
struct AiConfigResponse {
    enabled: bool,
    model: String,
    presets: &'static [crate::ai::AiPreset],
}

async fn config(State(state): State<AppState>, _user: AuthUser) -> Json<AiConfigResponse> {
    Json(AiConfigResponse {
        enabled: state.ai.enabled(),
        model: state.ai.model().to_string(),
        presets: &PRESETS,
    })
}

#[derive(Debug, Deserialize)]
struct InterpretRequest {
    job_id: Uuid,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecommendRequest {
    scenario_id: Uuid,
    objective: String,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReportRequest {
    job_id: Uuid,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PolishRequest {
    markdown: String,
    #[serde(default)]
    locale: Option<String>,
}

#[derive(Debug, Serialize)]
struct AiResponse {
    content: String,
    model: String,
    latency_ms: i64,
    request_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    suggestion: Option<Value>,
}

struct JobSummary {
    kind: String,
    summary: Value,
    preset: &'static str,
}

fn ensure_enabled(state: &AppState) -> Result<(), AppError> {
    if state.ai.enabled() {
        Ok(())
    } else {
        Err(AppError::BadRequest(
            "AI 功能未启用，请在后端配置 KST_AI_API_KEY 与 KST_AI_MODEL".into(),
        ))
    }
}

/// AI 调用统一前置校验：功能开关、写入角色、每日配额。
async fn guard_ai(state: &AppState, user: &AuthUser) -> Result<(), AppError> {
    ensure_enabled(state)?;
    user.require_writer()?;
    policy::enforce_ai_quota(&state.pool, user).await?;
    Ok(())
}

async fn load_job_summary(
    state: &AppState,
    user: &AuthUser,
    job_id: Uuid,
) -> Result<JobSummary, AppError> {
    let row: Option<(String, String, Option<Value>)> = sqlx::query_as(&format!(
        "SELECT j.kind, j.status, j.result FROM computation_jobs j
         WHERE j.id = $3 AND {scope}",
        scope = policy::JOB_READ_SCOPE
    ))
    .bind(user.id)
    .bind(user.org_id)
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await?;

    let (kind, status, result) = row.ok_or(AppError::NotFound("任务不存在"))?;
    if status != "succeeded" {
        return Err(AppError::BadRequest("仅已完成的任务支持 AI 解读".into()));
    }
    let result = result.ok_or(AppError::BadRequest("任务没有结果".into()))?;

    let (summary, preset) = match kind.as_str() {
        "access" => {
            let report: AccessReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("Access 结果解析失败：{err}")))?;
            (access_summary(&report), "access_interpret")
        }
        "coverage" => {
            let report: CoverageReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("覆盖结果解析失败：{err}")))?;
            (coverage_summary(&report), "coverage_interpret")
        }
        "environment" => {
            let report: EnvironmentReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("环境结果解析失败：{err}")))?;
            (environment_summary(&report), "environment_interpret")
        }
        "link_budget" => {
            let report: LinkBudgetReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("链路结果解析失败：{err}")))?;
            (link_summary(&report), "link_interpret")
        }
        "radar" => {
            let report: RadarReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("雷达结果解析失败：{err}")))?;
            (radar_summary(&report), "radar_interpret")
        }
        "constellation_analysis" => {
            let report: ConstellationAnalysisReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("星座分析结果解析失败：{err}")))?;
            (constellation_summary(&report), "coverage_interpret")
        }
        "interference" => {
            let report: InterferenceReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("干扰结果解析失败：{err}")))?;
            (interference_summary(&report), "link_interpret")
        }
        _ => {
            return Err(AppError::BadRequest(
                "仅 Access、覆盖、环境、链路、雷达、星座与干扰任务支持 AI 解读".into(),
            ))
        }
    };

    Ok(JobSummary {
        kind,
        summary,
        preset,
    })
}

fn access_summary(report: &AccessReport) -> Value {
    let preview: Vec<Value> = report
        .windows
        .iter()
        .take(12)
        .map(|window| {
            json!({
                "index": window.index,
                "start_utc": window.start_utc,
                "end_utc": window.end_utc,
                "duration_seconds": (window.duration_seconds * 10.0).round() / 10.0,
                "max_elevation_deg": (window.max_elevation_deg * 100.0).round() / 100.0,
            })
        })
        .collect();
    let average_duration = if report.windows.is_empty() {
        0.0
    } else {
        report.total_duration_seconds / report.windows.len() as f64
    };

    json!({
        "task": "satellite_ground_station_access",
        "satellite": report.satellite_name,
        "ground_station": report.ground_station_name,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "constraints": report.constraints,
        "min_elevation_deg": report.min_elevation_deg,
        "azimuth_range_deg": [report.azimuth_min_deg, report.azimuth_max_deg],
        "sensor": report.sensor_name,
        "window_count": report.window_count,
        "total_duration_seconds": (report.total_duration_seconds * 10.0).round() / 10.0,
        "longest_window_seconds": (report.longest_window_seconds * 10.0).round() / 10.0,
        "average_window_seconds": (average_duration * 10.0).round() / 10.0,
        "windows_preview": preview,
        "window_count_truncated": report.windows.len().saturating_sub(12),
    })
}

fn coverage_summary(report: &CoverageReport) -> Value {
    let mut best: Vec<&crate::orbit::CoveragePoint> = report.points.iter().collect();
    best.sort_by(|a, b| {
        b.visible_seconds
            .partial_cmp(&a.visible_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let best_points: Vec<Value> = best
        .iter()
        .take(6)
        .map(|point| {
            json!({
                "lat": point.latitude_deg,
                "lon": point.longitude_deg,
                "visible_seconds": (point.visible_seconds * 10.0).round() / 10.0,
                "access_count": point.access_count,
                "max_elevation_deg": (point.max_elevation_deg * 100.0).round() / 100.0,
            })
        })
        .collect();
    let gaps: Vec<Value> = report
        .points
        .iter()
        .filter(|point| !point.covered)
        .take(8)
        .map(|point| json!({ "lat": point.latitude_deg, "lon": point.longitude_deg }))
        .collect();

    json!({
        "task": "regional_coverage",
        "satellites": report.satellite_names,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "min_elevation_deg": report.min_elevation_deg,
        "region_deg": {
            "min_lat": report.region.min_latitude_deg,
            "max_lat": report.region.max_latitude_deg,
            "min_lon": report.region.min_longitude_deg,
            "max_lon": report.region.max_longitude_deg
        },
        "grid_step_deg": report.grid_step_deg,
        "total_points": report.total_points,
        "covered_points": report.covered_points,
        "coverage_ratio": (report.coverage_ratio * 10_000.0).round() / 10_000.0,
        "total_visible_seconds": (report.total_visible_seconds * 10.0).round() / 10.0,
        "average_visible_seconds": (report.average_visible_seconds * 10.0).round() / 10.0,
        "max_visible_seconds": (report.max_visible_seconds * 10.0).round() / 10.0,
        "best_points": best_points,
        "uncovered_sample": gaps,
        "uncovered_count": report.total_points.saturating_sub(report.covered_points),
    })
}

fn environment_summary(report: &EnvironmentReport) -> Value {
    json!({
        "task": "atmospheric_environment",
        "satellite": report.satellite_name,
        "model": report.model,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "activity": {
            "daily_f107_sfu": report.daily_f107_sfu,
            "mean81_f107_sfu": report.mean81_f107_sfu,
            "ap": report.ap
        },
        "spacecraft": {
            "mass_kg": report.mass_kg,
            "area_m2": report.area_m2,
            "drag_coefficient": report.drag_coefficient
        },
        "altitude_km": {
            "start": (report.start_altitude_km * 1000.0).round() / 1000.0,
            "end": (report.end_altitude_km * 1000.0).round() / 1000.0,
            "corrected_end": (report.corrected_end_altitude_km * 1000.0).round() / 1000.0
        },
        "density_kg_m3": {
            "mean": report.mean_density_kg_m3,
            "min": report.min_density_kg_m3,
            "max": report.max_density_kg_m3
        },
        "delta_v_m_s": (report.delta_v_m_s * 1000.0).round() / 1000.0,
        "estimated_altitude_loss_km": (report.estimated_altitude_loss_km * 1_000_000.0).round() / 1_000_000.0,
        "note": report.note,
        "sample_count": report.samples.len(),
    })
}

fn link_summary(report: &LinkBudgetReport) -> Value {
    let windows: Vec<Value> = report
        .windows
        .iter()
        .take(10)
        .map(|window| {
            json!({
                "index": window.index,
                "start_utc": window.start_utc,
                "end_utc": window.end_utc,
                "duration_seconds": (window.duration_seconds * 10.0).round() / 10.0,
                "min_margin_db": (window.min_margin_db * 100.0).round() / 100.0,
                "mean_margin_db": (window.mean_margin_db * 100.0).round() / 100.0,
                "min_range_km": (window.min_range_km * 100.0).round() / 100.0,
                "max_range_km": (window.max_range_km * 100.0).round() / 100.0,
            })
        })
        .collect();

    json!({
        "task": "link_budget",
        "satellite": report.satellite_name,
        "ground_station": report.ground_station_name,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "frequency_hz": report.frequency_hz,
        "bandwidth_hz": report.bandwidth_hz,
        "window_count": report.window_count,
        "total_duration_seconds": (report.total_duration_seconds * 10.0).round() / 10.0,
        "min_margin_db": (report.min_margin_db * 100.0).round() / 100.0,
        "mean_margin_db": (report.mean_margin_db * 100.0).round() / 100.0,
        "max_range_km": (report.max_range_km * 100.0).round() / 100.0,
        "mean_free_space_loss_db": (report.mean_free_space_loss_db * 100.0).round() / 100.0,
        "satellite_antenna_pattern": report.satellite_antenna_pattern,
        "atmosphere_model": report.atmosphere_model,
        "mean_atmospheric_loss_db": (report.mean_atmospheric_loss_db * 1000.0).round() / 1000.0,
        "max_atmospheric_loss_db": (report.max_atmospheric_loss_db * 1000.0).round() / 1000.0,
        "windows_preview": windows,
        "window_count_truncated": report.windows.len().saturating_sub(10),
    })
}

fn radar_summary(report: &RadarReport) -> Value {
    let windows: Vec<Value> = report
        .windows
        .iter()
        .take(10)
        .map(|window| {
            json!({
                "index": window.index,
                "start_utc": window.start_utc,
                "end_utc": window.end_utc,
                "duration_seconds": (window.duration_seconds * 10.0).round() / 10.0,
                "max_snr_db": (window.max_snr_db * 100.0).round() / 100.0,
                "detected_seconds": (window.detected_seconds * 10.0).round() / 10.0,
                "min_range_km": (window.min_range_km * 100.0).round() / 100.0,
                "max_range_km": (window.max_range_km * 100.0).round() / 100.0,
            })
        })
        .collect();

    json!({
        "task": "radar_detection",
        "satellite": report.satellite_name,
        "ground_station": report.ground_station_name,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "frequency_hz": report.frequency_hz,
        "bandwidth_hz": report.bandwidth_hz,
        "tx_power_w": report.tx_power_w,
        "required_snr_db": report.required_snr_db,
        "integration_gain_db": (report.integration_gain_db * 100.0).round() / 100.0,
        "max_detection_range_km": (report.max_detection_range_km * 100.0).round() / 100.0,
        "window_count": report.window_count,
        "detected_window_count": report.detected_window_count,
        "total_detection_seconds": (report.total_detection_seconds * 10.0).round() / 10.0,
        "min_range_km": (report.min_range_km * 100.0).round() / 100.0,
        "max_range_km": (report.max_range_km * 100.0).round() / 100.0,
        "windows_preview": windows,
        "window_count_truncated": report.windows.len().saturating_sub(10),
    })
}

fn constellation_summary(report: &ConstellationAnalysisReport) -> Value {
    let stations: Vec<Value> = report
        .station_stats
        .iter()
        .take(10)
        .map(|station| {
            json!({
                "station": station.station_name,
                "window_count": station.window_count,
                "total_access_seconds": (station.total_access_seconds * 10.0).round() / 10.0,
                "mean_gap_seconds": (station.mean_gap_seconds * 10.0).round() / 10.0,
                "max_gap_seconds": (station.max_gap_seconds * 10.0).round() / 10.0,
            })
        })
        .collect();

    json!({
        "task": "constellation_analysis",
        "satellite_count": report.satellite_ids.len(),
        "satellites": report.satellite_names,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "region_deg": {
            "min_lat": report.region.min_latitude_deg,
            "max_lat": report.region.max_latitude_deg,
            "min_lon": report.region.min_longitude_deg,
            "max_lon": report.region.max_longitude_deg
        },
        "grid_step_deg": report.grid_step_deg,
        "min_elevation_deg": report.min_elevation_deg,
        "total_points": report.total_points,
        "covered_points": report.covered_points,
        "coverage_ratio": (report.coverage_ratio * 10_000.0).round() / 10_000.0,
        "multi_covered_points": report.multi_covered_points,
        "mean_access_count": (report.mean_access_count * 100.0).round() / 100.0,
        "mean_revisit_seconds": (report.mean_revisit_seconds * 10.0).round() / 10.0,
        "max_gap_seconds": (report.max_gap_seconds * 10.0).round() / 10.0,
        "station_stats": stations,
        "note": report.note,
    })
}

fn interference_summary(report: &InterferenceReport) -> Value {
    let interferers: Vec<Value> = report
        .interferers
        .iter()
        .take(10)
        .map(|item| {
            json!({
                "satellite": item.satellite_name,
                "min_ci_db": (item.min_ci_db * 100.0).round() / 100.0,
                "mean_ci_db": (item.mean_ci_db * 100.0).round() / 100.0,
                "visible_samples": item.visible_samples,
            })
        })
        .collect();

    json!({
        "task": "interference_analysis",
        "desired_satellite": report.desired_satellite_name,
        "ground_station": report.ground_station_name,
        "window": { "start": report.start, "end": report.end, "step_seconds": report.step_seconds },
        "frequency_hz": report.frequency_hz,
        "threshold_ci_db": report.threshold_ci_db,
        "sample_count": report.sample_count,
        "degraded_samples": report.degraded_samples,
        "degraded_ratio": (report.degraded_ratio * 10_000.0).round() / 10_000.0,
        "min_ci_db": (report.min_ci_db * 100.0).round() / 100.0,
        "mean_ci_db": (report.mean_ci_db * 100.0).round() / 100.0,
        "interferers": interferers,
    })
}

fn scenario_summary(scenario: &ScenarioData) -> Value {
    let satellites: Vec<Value> = scenario
        .satellites
        .iter()
        .map(|item| {
            json!({
                "name": item.name,
                "norad_id": item.norad_id,
                "enabled": item.enabled,
            })
        })
        .collect();
    let stations: Vec<Value> = scenario
        .ground_stations
        .iter()
        .map(|item| {
            json!({
                "name": item.name,
                "latitude_deg": item.latitude_deg,
                "longitude_deg": item.longitude_deg,
                "altitude_km": item.altitude_km,
                "min_elevation_deg": item.min_elevation_deg,
                "enabled": item.enabled,
            })
        })
        .collect();
    let sensors: Vec<Value> = scenario
        .sensors
        .iter()
        .map(|item| {
            json!({
                "name": item.name,
                "satellite": scenario.satellite(&item.satellite_id).map(|sat| sat.name.clone()),
                "shape": item.shape,
                "half_angle_deg": item.half_angle_deg,
                "horizontal_fov_deg": item.horizontal_fov_deg,
                "vertical_fov_deg": item.vertical_fov_deg,
                "pointing_mode": item.pointing_mode,
                "max_range_km": item.max_range_km,
                "enabled": item.enabled,
            })
        })
        .collect();

    json!({
        "satellites": satellites,
        "ground_stations": stations,
        "sensors": sensors,
    })
}

fn parse_json_object(content: &str) -> Option<Value> {
    let trimmed = content.trim();
    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let without_fence = without_fence
        .strip_suffix("```")
        .unwrap_or(without_fence)
        .trim();
    serde_json::from_str::<Value>(without_fence)
        .ok()
        .filter(Value::is_object)
}

fn push_line(out: &mut String, label: &str, value: &Value) {
    if let Some(number) = value.as_f64() {
        out.push_str(&format!("- {label}: {number}\n"));
    } else if let Some(text) = value.as_str() {
        out.push_str(&format!("- {label}: {text}\n"));
    }
}

fn render_suggestion(value: &Value, english: bool) -> String {
    let mut out = String::new();
    if let Some(summary) = value.get("summary").and_then(Value::as_str) {
        out.push_str(if english { "### Summary\n" } else { "### 推荐结论\n" });
        out.push_str(summary);
        out.push_str("\n\n");
    }

    if let Some(orbit) = value.get("orbit").filter(|item| item.is_object()) {
        out.push_str(if english {
            "### Orbit parameters\n"
        } else {
            "### 轨道参数\n"
        });
        push_line(
            &mut out,
            if english { "Altitude (km)" } else { "轨道高度 (km)" },
            &orbit["altitude_km"],
        );
        push_line(
            &mut out,
            if english { "Inclination (deg)" } else { "倾角 (deg)" },
            &orbit["inclination_deg"],
        );
        push_line(
            &mut out,
            if english { "Eccentricity" } else { "偏心率" },
            &orbit["eccentricity"],
        );
        out.push('\n');
    }

    if let Some(sensor) = value.get("sensor").filter(|item| item.is_object()) {
        out.push_str(if english {
            "### Sensor parameters\n"
        } else {
            "### 传感器参数\n"
        });
        push_line(
            &mut out,
            if english { "Shape" } else { "视场形状" },
            &sensor["shape"],
        );
        push_line(
            &mut out,
            if english { "Half angle (deg)" } else { "半视场角 (deg)" },
            &sensor["half_angle_deg"],
        );
        push_line(
            &mut out,
            if english { "Max range (km)" } else { "最大作用距离 (km)" },
            &sensor["max_range_km"],
        );
        push_line(
            &mut out,
            if english { "Pointing mode" } else { "指向模式" },
            &sensor["pointing_mode"],
        );
        out.push('\n');
    }

    if let Some(constellation) = value.get("constellation").filter(|item| item.is_object()) {
        out.push_str(if english {
            "### Constellation parameters\n"
        } else {
            "### 星座参数\n"
        });
        push_line(
            &mut out,
            if english { "Pattern" } else { "构型" },
            &constellation["constellation_type"],
        );
        push_line(
            &mut out,
            if english { "Planes" } else { "轨道面数" },
            &constellation["planes"],
        );
        push_line(
            &mut out,
            if english { "Total satellites" } else { "卫星总数" },
            &constellation["total_satellites"],
        );
        push_line(
            &mut out,
            if english { "Phasing" } else { "相位因子" },
            &constellation["phasing"],
        );
        out.push('\n');
    }

    if let Some(risks) = value.get("risks").and_then(Value::as_array) {
        if !risks.is_empty() {
            out.push_str(if english { "### Risks\n" } else { "### 风险提示\n" });
            for risk in risks {
                if let Some(text) = risk.as_str() {
                    out.push_str(&format!("- {text}\n"));
                }
            }
        }
    }

    if out.trim().is_empty() {
        serde_json::to_string_pretty(value).unwrap_or_default()
    } else {
        out
    }
}

fn language_instruction(locale: Option<&str>) -> &'static str {
    if locale.unwrap_or("zh").to_ascii_lowercase().starts_with("en") {
        "Answer in English. Use Markdown with clear headings and bullet points."
    } else {
        "请使用简体中文回答，使用 Markdown 标题和要点，不要编造未提供的数据。"
    }
}

async fn record_start(
    state: &AppState,
    user: &AuthUser,
    kind: &str,
    preset: &str,
    job_id: Option<Uuid>,
    scenario_id: Option<Uuid>,
    model: &str,
) -> Option<Uuid> {
    let id = Uuid::new_v4();
    let result = sqlx::query(
        "INSERT INTO ai_requests
            (id, org_id, created_by, kind, preset, job_id, scenario_id, model, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'running')",
    )
    .bind(id)
    .bind(user.org_id)
    .bind(user.id)
    .bind(kind)
    .bind(preset)
    .bind(job_id)
    .bind(scenario_id)
    .bind(model)
    .execute(&state.pool)
    .await;
    match result {
        Ok(_) => Some(id),
        Err(err) => {
            tracing::warn!(error = %err, "AI 请求记录写入失败");
            None
        }
    }
}

async fn record_finish(
    state: &AppState,
    id: Option<Uuid>,
    error: Option<&str>,
    latency_ms: Option<i64>,
) {
    let Some(id) = id else { return };
    let status = if error.is_some() { "failed" } else { "succeeded" };
    let _ = sqlx::query(
        "UPDATE ai_requests
         SET status = $2, error = $3, latency_ms = $4, finished_at = now()
         WHERE id = $1",
    )
    .bind(id)
    .bind(status)
    .bind(error)
    .bind(latency_ms)
    .execute(&state.pool)
    .await;
}

async fn interpret(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<InterpretRequest>,
) -> Result<Json<AiResponse>, AppError> {
    guard_ai(&state, &user).await?;
    let summary = load_job_summary(&state, &user, payload.job_id).await?;
    let request_id = record_start(
        &state,
        &user,
        &summary.kind,
        summary.preset,
        Some(payload.job_id),
        None,
        state.ai.model(),
    )
    .await;

    let system = if language_instruction(payload.locale.as_deref()).starts_with("Please") {
        "You are a senior space mission analyst. Interpret the computation results, highlight key findings and actionable risks. Do not invent data."
    } else {
        "你是一位资深航天任务分析专家。请基于给定的计算结果做解读，突出关键结论、约束影响与可执行建议，不要编造未提供的数据。"
    };
    let user_prompt = format!(
        "{}\n\n以下是后端计算的摘要数据（JSON）：\n```json\n{}\n```",
        language_instruction(payload.locale.as_deref()),
        serde_json::to_string_pretty(&summary.summary).unwrap_or_default()
    );

    match state.ai.complete(system, &user_prompt).await {
        Ok((content, latency_ms)) => {
            record_finish(&state, request_id, None, Some(latency_ms)).await;
            Ok(Json(AiResponse {
                content,
                model: state.ai.model().to_string(),
                latency_ms,
                request_id: request_id.unwrap_or_else(Uuid::nil),
                suggestion: None,
            }))
        }
        Err(error) => {
            record_finish(&state, request_id, Some(&error), None).await;
            Err(AppError::BadRequest(error))
        }
    }
}

async fn recommend(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<RecommendRequest>,
) -> Result<Json<AiResponse>, AppError> {
    guard_ai(&state, &user).await?;
    let objective = payload.objective.trim();
    if objective.is_empty() {
        return Err(AppError::BadRequest("请填写任务目标".into()));
    }
    if objective.chars().count() > 500 {
        return Err(AppError::BadRequest("任务目标不能超过 500 字".into()));
    }

    let data: Option<Value> = sqlx::query_scalar(&format!(
        "SELECT data FROM scenarios WHERE id = $3 AND ((org_id IS NOT NULL AND org_id = $1) OR (org_id IS NULL AND created_by = $2))"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(payload.scenario_id)
    .fetch_optional(&state.pool)
    .await?;
    let data = data.ok_or(AppError::NotFound("场景不存在"))?;
    let scenario: ScenarioData = serde_json::from_value(data)
        .map_err(|err| AppError::Internal(format!("场景数据损坏：{err}")))?;

    let request_id = record_start(
        &state,
        &user,
        "recommendation",
        "orbit_recommend",
        None,
        Some(payload.scenario_id),
        state.ai.model(),
    )
    .await;

    let english = language_instruction(payload.locale.as_deref()).starts_with("Please");
    let system = if english {
        "You are a senior space mission architect. Return ONLY a single JSON object, no Markdown, no explanation. Schema: {\"summary\": string, \"orbit\": {\"altitude_km\": number, \"inclination_deg\": number, \"eccentricity\": number}, \"sensor\": {\"shape\": \"conical\"|\"rectangular\", \"half_angle_deg\": number, \"max_range_km\": number, \"pointing_mode\": \"nadir\"|\"fixed\"|\"target\"}, \"constellation\": {\"constellation_type\": \"walker_delta\"|\"walker_star\"|\"streets\", \"planes\": integer, \"total_satellites\": integer, \"phasing\": integer}, \"risks\": [string]}. Do not invent TLE data."
    } else {
        "你是一位资深航天任务总体设计师。只返回一个 JSON 对象，不要输出 Markdown 代码块或解释。结构为：{\"summary\": \"一句话结论\", \"orbit\": {\"altitude_km\": 数值, \"inclination_deg\": 数值, \"eccentricity\": 数值}, \"sensor\": {\"shape\": \"conical\" 或 \"rectangular\", \"half_angle_deg\": 数值, \"max_range_km\": 数值, \"pointing_mode\": \"nadir\" 或 \"fixed\" 或 \"target\"}, \"constellation\": {\"constellation_type\": \"walker_delta\" 或 \"walker_star\" 或 \"streets\", \"planes\": 整数, \"total_satellites\": 整数, \"phasing\": 整数}, \"risks\": [\"风险\"]}。不要编造 TLE 数据。"
    };
    let user_prompt = format!(
        "{}\n\n任务目标：{}\n\n当前场景对象摘要（JSON）：\n```json\n{}\n```\n\n请基于任务目标与当前场景给出可直接应用的参数建议，数值必须合理且落在工程可行范围内。",
        language_instruction(payload.locale.as_deref()),
        objective,
        serde_json::to_string_pretty(&scenario_summary(&scenario)).unwrap_or_default()
    );

    match state.ai.complete_json(system, &user_prompt).await {
        Ok((content, latency_ms)) => {
            record_finish(&state, request_id, None, Some(latency_ms)).await;
            let suggestion = parse_json_object(&content);
            let rendered = match suggestion.as_ref() {
                Some(value) => render_suggestion(value, english),
                None => content.clone(),
            };
            Ok(Json(AiResponse {
                content: rendered,
                model: state.ai.model().to_string(),
                latency_ms,
                request_id: request_id.unwrap_or_else(Uuid::nil),
                suggestion,
            }))
        }
        Err(error) => {
            record_finish(&state, request_id, Some(&error), None).await;
            Err(AppError::BadRequest(error))
        }
    }
}

async fn report(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<ReportRequest>,
) -> Result<Json<AiResponse>, AppError> {
    guard_ai(&state, &user).await?;
    let summary = load_job_summary(&state, &user, payload.job_id).await?;
    let request_id = record_start(
        &state,
        &user,
        &summary.kind,
        "report_generate",
        Some(payload.job_id),
        None,
        state.ai.model(),
    )
    .await;

    let english = language_instruction(payload.locale.as_deref()).starts_with("Please");
    let system = if english {
        "You are a senior space mission analyst writing a formal computation report in Markdown."
    } else {
        "你是一位资深航天任务分析专家，需要基于计算结果撰写正式的 Markdown 报告。"
    };
    let user_prompt = format!(
        "{}\n\n请基于以下后端计算结果撰写报告，包含：任务概述、方法与约束、关键结果、风险与建议、结论。结尾注明“本报告由 AI 生成，需人工复核”。\n\n```json\n{}\n```",
        language_instruction(payload.locale.as_deref()),
        serde_json::to_string_pretty(&summary.summary).unwrap_or_default()
    );

    match state.ai.complete(system, &user_prompt).await {
        Ok((content, latency_ms)) => {
            record_finish(&state, request_id, None, Some(latency_ms)).await;
            Ok(Json(AiResponse {
                content,
                model: state.ai.model().to_string(),
                latency_ms,
                request_id: request_id.unwrap_or_else(Uuid::nil),
                suggestion: None,
            }))
        }
        Err(error) => {
            record_finish(&state, request_id, Some(&error), None).await;
            Err(AppError::BadRequest(error))
        }
    }
}

async fn polish(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<PolishRequest>,
) -> Result<Json<AiResponse>, AppError> {
    guard_ai(&state, &user).await?;
    let markdown = payload.markdown.trim();
    if markdown.is_empty() {
        return Err(AppError::BadRequest("待润色的报告内容为空".into()));
    }
    if markdown.chars().count() > 30_000 {
        return Err(AppError::BadRequest("报告内容过长，无法进行 AI 润色".into()));
    }

    let request_id = record_start(
        &state,
        &user,
        "report_polish",
        "report_polish",
        None,
        None,
        state.ai.model(),
    )
    .await;

    let english = language_instruction(payload.locale.as_deref()).starts_with("Please");
    let system = if english {
        "You are a senior space mission analyst. Polish the provided Markdown report: improve structure, clarity and professionalism, but never invent or alter numeric results."
    } else {
        "你是一位资深航天任务分析专家。请在完全不改动任何数值结果的前提下润色给定 Markdown 报告，提升结构、可读性与专业表达；不要编造数据，不要删除表格。"
    };
    let user_prompt = format!(
        "{}\n\n请直接输出润色后的完整 Markdown 报告，不要添加解释性前言。\n\n```markdown\n{}\n```",
        language_instruction(payload.locale.as_deref()),
        markdown
    );

    match state.ai.complete(system, &user_prompt).await {
        Ok((content, latency_ms)) => {
            record_finish(&state, request_id, None, Some(latency_ms)).await;
            Ok(Json(AiResponse {
                content,
                model: state.ai.model().to_string(),
                latency_ms,
                request_id: request_id.unwrap_or_else(Uuid::nil),
                suggestion: None,
            }))
        }
        Err(error) => {
            record_finish(&state, request_id, Some(&error), None).await;
            Err(AppError::BadRequest(error))
        }
    }
}

/// 保留给未来的请求列表接口类型，避免重复定义。
#[allow(dead_code)]
#[derive(Debug, Serialize)]
struct AiRequestView {
    id: Uuid,
    kind: String,
    preset: String,
    status: String,
    error: Option<String>,
    latency_ms: Option<i32>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct SaveInterpretationRequest {
    job_id: Uuid,
    #[serde(default)]
    preset: String,
    content: String,
    #[serde(default)]
    model: String,
}

#[derive(Debug, Deserialize)]
struct InterpretationQuery {
    job_id: Option<Uuid>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct InterpretationView {
    id: Uuid,
    job_id: Option<Uuid>,
    scenario_id: Option<Uuid>,
    preset: String,
    model: String,
    content: String,
    created_by: Option<Uuid>,
    username: Option<String>,
    created_at: DateTime<Utc>,
}

const INTERPRETATION_COLUMNS: &str = "i.id, i.job_id, i.scenario_id, i.preset, i.model, i.content,
     i.created_by, u.username, i.created_at";

async fn job_visible(state: &AppState, user: &AuthUser, job_id: Uuid) -> Result<bool, AppError> {
    let row: Option<(Uuid,)> = sqlx::query_as(&format!(
        "SELECT j.id FROM computation_jobs j WHERE j.id = $3 AND {scope}",
        scope = policy::JOB_READ_SCOPE
    ))
    .bind(user.id)
    .bind(user.org_id)
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(row.is_some())
}

async fn save_interpretation(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<SaveInterpretationRequest>,
) -> Result<Json<InterpretationView>, AppError> {
    user.require_writer()?;
    if payload.content.trim().is_empty() {
        return Err(AppError::BadRequest("解读内容为空".into()));
    }
    if !job_visible(&state, &user, payload.job_id).await? {
        return Err(AppError::NotFound("任务不存在"));
    }

    let job: (Uuid, String, Option<Uuid>) = sqlx::query_as(
        "SELECT scenario_id, kind, org_id FROM computation_jobs WHERE id = $1",
    )
    .bind(payload.job_id)
    .fetch_one(&state.pool)
    .await?;
    let (scenario_id, kind, org_id) = job;
    let preset = if payload.preset.trim().is_empty() {
        format!("{kind}_interpret")
    } else {
        payload.preset.trim().to_string()
    };

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO ai_interpretations
            (id, org_id, created_by, job_id, scenario_id, preset, model, content)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(org_id.or(user.org_id))
    .bind(user.id)
    .bind(payload.job_id)
    .bind(scenario_id)
    .bind(&preset)
    .bind(payload.model.trim())
    .bind(payload.content.trim())
    .execute(&state.pool)
    .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::INTERPRETATION_SAVE,
        "job",
        Some(payload.job_id),
        serde_json::json!({ "interpretation_id": id, "preset": preset }),
    )
    .await;

    let row: InterpretationView = sqlx::query_as(&format!(
        "SELECT {INTERPRETATION_COLUMNS}
         FROM ai_interpretations i
         LEFT JOIN users u ON u.id = i.created_by
         WHERE i.id = $1"
    ))
    .bind(id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}

async fn list_interpretations(
    State(state): State<AppState>,
    user: AuthUser,
    axum::extract::Query(query): axum::extract::Query<InterpretationQuery>,
) -> Result<Json<Vec<InterpretationView>>, AppError> {
    let limit = query.limit.unwrap_or(20).clamp(1, 100);

    let rows: Vec<InterpretationView> = if let Some(job_id) = query.job_id {
        if !job_visible(&state, &user, job_id).await? {
            return Err(AppError::NotFound("任务不存在"));
        }
        sqlx::query_as(&format!(
            "SELECT {INTERPRETATION_COLUMNS}
             FROM ai_interpretations i
             LEFT JOIN users u ON u.id = i.created_by
             WHERE i.job_id = $1
             ORDER BY i.created_at DESC
             LIMIT $2"
        ))
        .bind(job_id)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    } else {
        sqlx::query_as(&format!(
            "SELECT {INTERPRETATION_COLUMNS}
             FROM ai_interpretations i
             LEFT JOIN users u ON u.id = i.created_by
             WHERE i.created_by = $1 OR (i.org_id IS NOT NULL AND i.org_id = $2)
             ORDER BY i.created_at DESC
             LIMIT $3"
        ))
        .bind(user.id)
        .bind(user.org_id)
        .bind(limit)
        .fetch_all(&state.pool)
        .await?
    };

    Ok(Json(rows))
}
