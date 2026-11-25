use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::AppError,
    orbit::{self, AccessReport},
    scenario::ScenarioData,
    state::AppState,
};

const SCOPE: &str =
    "((org_id IS NOT NULL AND org_id = $1) OR (org_id IS NULL AND created_by = $2))";
const MAX_SAMPLES: f64 = 20_000.0;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_jobs).post(create_job))
        .route("/{id}", get(get_job))
        .route("/{id}/csv", get(job_csv))
}

#[derive(Debug, sqlx::FromRow)]
struct JobRow {
    id: Uuid,
    scenario_id: Uuid,
    kind: String,
    status: String,
    params: Value,
    result: Option<Value>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
struct JobView {
    id: Uuid,
    scenario_id: Uuid,
    kind: String,
    status: String,
    params: Value,
    result: Option<Value>,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
}

impl JobRow {
    fn into_view(self) -> JobView {
        JobView {
            id: self.id,
            scenario_id: self.scenario_id,
            kind: self.kind,
            status: self.status,
            params: self.params,
            result: self.result,
            error: self.error,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CreateJobRequest {
    scenario_id: Uuid,
    kind: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JobQuery {
    scenario_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct PropagationParams {
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    satellite_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct AccessParams {
    satellite_id: String,
    ground_station_id: String,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    min_elevation_deg: Option<f64>,
}

fn default_step() -> f64 {
    30.0
}

fn parse_time(raw: &str, label: &str) -> Result<DateTime<Utc>, AppError> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|_| AppError::BadRequest(format!("{label} 需为 RFC3339 格式时间")))
}

fn validate_window(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step_seconds: f64,
) -> Result<(), AppError> {
    let total_seconds = (end - start).num_milliseconds() as f64 / 1000.0;
    if total_seconds <= 0.0 {
        return Err(AppError::BadRequest("结束时间必须晚于开始时间".into()));
    }
    if !step_seconds.is_finite() || !(1.0..=3600.0).contains(&step_seconds) {
        return Err(AppError::BadRequest("采样步长需在 [1, 3600] 秒内".into()));
    }
    if total_seconds / step_seconds > MAX_SAMPLES {
        return Err(AppError::BadRequest(
            "采样点过多，请增大步长或缩短时间范围".into(),
        ));
    }
    Ok(())
}

fn validate_params(kind: &str, params: &Value) -> Result<(), AppError> {
    match kind {
        "propagation" => {
            let params: PropagationParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("传播参数无效：{err}")))?;
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "access" => {
            let params: AccessParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("Access 参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() || params.ground_station_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择卫星与地面站".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        _ => Err(AppError::BadRequest(format!("未知任务类型：{kind}"))),
    }
}

async fn scenario_exists(state: &AppState, user: &AuthUser, scenario_id: Uuid) -> Result<bool, AppError> {
    let exists: Option<Uuid> = sqlx::query_scalar(&format!(
        "SELECT id FROM scenarios WHERE id = $3 AND {SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(scenario_id)
    .fetch_optional(&state.pool)
    .await?;
    Ok(exists.is_some())
}

async fn create_job(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateJobRequest>,
) -> Result<Json<JobView>, AppError> {
    if !scenario_exists(&state, &user, payload.scenario_id).await? {
        return Err(AppError::NotFound("场景不存在"));
    }
    validate_params(&payload.kind, &payload.params)?;

    let job_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO computation_jobs (id, org_id, created_by, scenario_id, kind, status, params)
         VALUES ($1, $2, $3, $4, $5, 'queued', $6)",
    )
    .bind(job_id)
    .bind(user.org_id)
    .bind(user.id)
    .bind(payload.scenario_id)
    .bind(&payload.kind)
    .bind(&payload.params)
    .execute(&state.pool)
    .await?;

    let background = state.clone();
    tokio::spawn(async move {
        execute_job(background, job_id).await;
    });

    let row: JobRow = sqlx::query_as(
        "SELECT id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at
         FROM computation_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row.into_view()))
}

async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<JobQuery>,
) -> Result<Json<Vec<JobView>>, AppError> {
    let rows: Vec<JobRow> = sqlx::query_as(&format!(
        "SELECT id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at
         FROM computation_jobs
         WHERE {SCOPE} AND ($3::uuid IS NULL OR scenario_id = $3)
         ORDER BY created_at DESC
         LIMIT 50"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(query.scenario_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows.into_iter().map(JobRow::into_view).collect()))
}

async fn get_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, AppError> {
    let row: Option<JobRow> = sqlx::query_as(&format!(
        "SELECT id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at
         FROM computation_jobs WHERE id = $3 AND {SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let row = row.ok_or(AppError::NotFound("任务不存在"))?;
    Ok(Json(row.into_view()))
}

async fn job_csv(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let row: Option<JobRow> = sqlx::query_as(&format!(
        "SELECT id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at
         FROM computation_jobs WHERE id = $3 AND {SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let row = row.ok_or(AppError::NotFound("任务不存在"))?;
    if row.kind != "access" {
        return Err(AppError::BadRequest("仅 Access 任务支持 CSV 导出".into()));
    }
    if row.status != "succeeded" {
        return Err(AppError::BadRequest("任务尚未完成，无法导出".into()));
    }
    let result = row
        .result
        .ok_or(AppError::BadRequest("任务没有结果".into()))?;
    let report: AccessReport = serde_json::from_value(result)
        .map_err(|err| AppError::Internal(format!("Access 结果解析失败：{err}")))?;

    let csv = access_csv(&report);
    Ok((
        [
            (
                header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"access-{id}.csv\""),
            ),
        ],
        csv,
    )
        .into_response())
}

fn access_csv(report: &AccessReport) -> String {
    let mut output = String::from(
        "\u{feff}window,satellite,ground_station,min_elevation_deg,start_utc,end_utc,duration_seconds,max_elevation_deg\n",
    );
    for window in &report.windows {
        output.push_str(&format!(
            "{},{},{},{:.2},{},{},{:.1},{:.3}\n",
            window.index,
            csv_field(&report.satellite_name),
            csv_field(&report.ground_station_name),
            report.min_elevation_deg,
            window.start_utc,
            window.end_utc,
            window.duration_seconds,
            window.max_elevation_deg
        ));
    }
    output
}

fn csv_field(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

async fn execute_job(state: AppState, job_id: Uuid) {
    if let Err(error) = run_job(state.clone(), job_id).await {
        tracing::error!(%job_id, error = %error, "计算任务失败");
        let _ = sqlx::query(
            "UPDATE computation_jobs
             SET status = 'failed', error = $2, finished_at = now()
             WHERE id = $1",
        )
        .bind(job_id)
        .bind(&error)
        .execute(&state.pool)
        .await;
    }
}

async fn run_job(state: AppState, job_id: Uuid) -> Result<(), String> {
    let row: Option<JobRow> = sqlx::query_as(
        "SELECT id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at
         FROM computation_jobs WHERE id = $1",
    )
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| err.to_string())?;
    let row = row.ok_or_else(|| "任务不存在".to_string())?;

    sqlx::query("UPDATE computation_jobs SET status = 'running', started_at = now() WHERE id = $1")
        .bind(job_id)
        .execute(&state.pool)
        .await
        .map_err(|err| err.to_string())?;

    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM scenarios WHERE id = $1")
        .bind(row.scenario_id)
        .fetch_optional(&state.pool)
        .await
        .map_err(|err| err.to_string())?;
    let data = data.ok_or_else(|| "场景不存在".to_string())?;
    let scenario: ScenarioData =
        serde_json::from_value(data).map_err(|err| format!("场景数据损坏：{err}"))?;

    let kind = row.kind.clone();
    let params = row.params.clone();

    let result = tokio::task::spawn_blocking(move || compute_job_result(&kind, &params, &scenario))
        .await
        .map_err(|err| format!("任务线程异常：{err}"))??;

    sqlx::query(
        "UPDATE computation_jobs
         SET status = 'succeeded', result = $2, error = NULL, finished_at = now()
         WHERE id = $1",
    )
    .bind(job_id)
    .bind(result)
    .execute(&state.pool)
    .await
    .map_err(|err| err.to_string())?;

    Ok(())
}

fn compute_job_result(kind: &str, params: &Value, scenario: &ScenarioData) -> Result<Value, String> {
    match kind {
        "propagation" => {
            let params: PropagationParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("传播参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = orbit::propagate_scenario(
                scenario,
                start,
                end,
                params.step_seconds,
                params.satellite_ids.as_deref(),
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "access" => {
            let params: AccessParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("Access 参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = orbit::compute_access(
                scenario,
                &params.satellite_id,
                &params.ground_station_id,
                start,
                end,
                params.step_seconds,
                params.min_elevation_deg,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        other => Err(format!("未知任务类型：{other}")),
    }
}
