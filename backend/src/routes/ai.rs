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
    orbit::{AccessReport, CoverageReport},
    scenario::ScenarioData,
    state::AppState,
};

const JOB_SCOPE: &str =
    "((org_id IS NOT NULL AND org_id = $1) OR (org_id IS NULL AND created_by = $2))";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/config", get(config))
        .route("/interpret", post(interpret))
        .route("/recommend", post(recommend))
        .route("/report", post(report))
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

#[derive(Debug, Serialize)]
struct AiResponse {
    content: String,
    model: String,
    latency_ms: i64,
    request_id: Uuid,
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

async fn load_job_summary(
    state: &AppState,
    user: &AuthUser,
    job_id: Uuid,
) -> Result<JobSummary, AppError> {
    let row: Option<(String, String, Option<Value>)> = sqlx::query_as(&format!(
        "SELECT kind, status, result FROM computation_jobs WHERE id = $3 AND {JOB_SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
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
        _ => {
            return Err(AppError::BadRequest(
                "仅 Access、覆盖与环境任务支持 AI 解读".into(),
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
    ensure_enabled(&state)?;
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
    ensure_enabled(&state)?;
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
        "You are a senior space mission architect. Recommend concrete orbit altitude, inclination, sensor FOV and pointing parameters, with trade-offs and risks. Do not invent TLE data."
    } else {
        "你是一位资深航天任务总体设计师。请给出具体的轨道高度、倾角、传感器视场与指向参数建议，并说明权衡与风险。不要编造 TLE 数据。"
    };
    let user_prompt = format!(
        "{}\n\n任务目标：{}\n\n当前场景对象摘要（JSON）：\n```json\n{}\n```\n\n请输出：1) 推荐结论；2) 参数建议表；3) 依据；4) 风险与验证建议。",
        language_instruction(payload.locale.as_deref()),
        objective,
        serde_json::to_string_pretty(&scenario_summary(&scenario)).unwrap_or_default()
    );

    match state.ai.complete(system, &user_prompt).await {
        Ok((content, latency_ms)) => {
            record_finish(&state, request_id, None, Some(latency_ms)).await;
            Ok(Json(AiResponse {
                content,
                model: state.ai.model().to_string(),
                latency_ms,
                request_id: request_id.unwrap_or_else(Uuid::nil),
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
    ensure_enabled(&state)?;
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
