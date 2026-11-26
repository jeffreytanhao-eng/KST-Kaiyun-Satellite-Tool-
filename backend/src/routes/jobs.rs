use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    environment::{self, EnvironmentOptions, EnvironmentReport},
    error::AppError,
    link::{self, InterferenceReport, LinkBudgetReport, LinkOptions},
    orbit::{
        self, AccessOptions, AccessReport, ConstellationAnalysisReport, CoverageRegion,
        CoverageReport, PropagationCompareReport,
    },
    policy::{self, actions},
    radar::{self, RadarOptions, RadarReport},
    report::{self, ReportDocument, ReportRequest},
    scenario::ScenarioData,
    state::AppState,
};

const MAX_SAMPLES: f64 = 20_000.0;

const JOB_COLUMNS: &str = "id, scenario_id, kind, status, params, result, error, created_at, started_at, finished_at, progress, cached, cancel_requested, cache_key";

/// 列表查询不返回 params/result 等大字段，避免轮询时反复传输与序列化兆级 JSON。
const JOB_SUMMARY_COLUMNS: &str = "id, scenario_id, kind, status, error, created_at, started_at, finished_at, progress, cached, cancel_requested";

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_jobs).post(create_job))
        .route("/{id}", get(get_job))
        .route("/{id}/cancel", post(cancel_job))
        .route("/{id}/csv", get(job_csv))
        .route("/{id}/report", get(job_report))
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
    progress: i32,
    cached: bool,
    cancel_requested: bool,
    cache_key: Option<String>,
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
    progress: i32,
    cached: bool,
    cancel_requested: bool,
    cache_key: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct JobSummaryRow {
    id: Uuid,
    scenario_id: Uuid,
    kind: String,
    status: String,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    progress: i32,
    cached: bool,
    cancel_requested: bool,
}

#[derive(Debug, Serialize)]
struct JobSummaryView {
    id: Uuid,
    scenario_id: Uuid,
    kind: String,
    status: String,
    error: Option<String>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    progress: i32,
    cached: bool,
    cancel_requested: bool,
}

impl JobSummaryRow {
    fn into_view(self) -> JobSummaryView {
        JobSummaryView {
            id: self.id,
            scenario_id: self.scenario_id,
            kind: self.kind,
            status: self.status,
            error: self.error,
            created_at: self.created_at,
            started_at: self.started_at,
            finished_at: self.finished_at,
            progress: self.progress,
            cached: self.cached,
            cancel_requested: self.cancel_requested,
        }
    }
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
            progress: self.progress,
            cached: self.cached,
            cancel_requested: self.cancel_requested,
            cache_key: self.cache_key,
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
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ReportDownloadQuery {
    format: Option<String>,
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
    #[serde(default)]
    azimuth_min_deg: Option<f64>,
    #[serde(default)]
    azimuth_max_deg: Option<f64>,
    #[serde(default)]
    sensor_id: Option<String>,
}

impl AccessParams {
    fn options(&self) -> AccessOptions {
        AccessOptions {
            min_elevation_deg: self.min_elevation_deg,
            azimuth_min_deg: self.azimuth_min_deg,
            azimuth_max_deg: self.azimuth_max_deg,
            sensor_id: self.sensor_id.clone(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CoverageParams {
    #[serde(default)]
    satellite_ids: Option<Vec<String>>,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    region: CoverageRegion,
    #[serde(default = "default_grid_step")]
    grid_step_deg: f64,
    #[serde(default = "default_min_elevation")]
    min_elevation_deg: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct EnvironmentParams {
    satellite_id: String,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    options: EnvironmentOptions,
}

#[derive(Debug, Clone, Deserialize)]
struct CompareParams {
    satellite_id: String,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct LinkParams {
    satellite_id: String,
    ground_station_id: String,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    options: LinkOptions,
}

#[derive(Debug, Clone, Deserialize)]
struct ConstellationAnalysisParams {
    #[serde(default)]
    satellite_ids: Option<Vec<String>>,
    #[serde(default)]
    station_ids: Option<Vec<String>>,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    region: CoverageRegion,
    #[serde(default = "default_grid_step")]
    grid_step_deg: f64,
    #[serde(default = "default_min_elevation")]
    min_elevation_deg: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct RadarParams {
    satellite_id: String,
    ground_station_id: String,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    options: RadarOptions,
}

#[derive(Debug, Clone, Deserialize)]
struct InterferenceParams {
    satellite_id: String,
    ground_station_id: String,
    #[serde(default)]
    interferer_ids: Vec<String>,
    start: String,
    end: String,
    #[serde(default = "default_step")]
    step_seconds: f64,
    #[serde(default)]
    options: LinkOptions,
    #[serde(default = "default_ci_threshold")]
    threshold_ci_db: f64,
}

fn default_ci_threshold() -> f64 {
    10.0
}

fn default_step() -> f64 {
    30.0
}

fn default_grid_step() -> f64 {
    2.0
}

fn default_min_elevation() -> f64 {
    5.0
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
            match (params.azimuth_min_deg, params.azimuth_max_deg) {
                (Some(minimum), Some(maximum)) => {
                    if !(0.0..=360.0).contains(&minimum)
                        || !(0.0..=360.0).contains(&maximum)
                        || (minimum - maximum).abs() < 1e-9
                    {
                        return Err(AppError::BadRequest(
                            "方位角范围需为 [0, 360] 度内的两个不同值".into(),
                        ));
                    }
                }
                (None, None) => {}
                _ => return Err(AppError::BadRequest("方位角范围需同时填写上下限".into())),
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "coverage" => {
            let params: CoverageParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("覆盖参数无效：{err}")))?;
            if !(0.1..=30.0).contains(&params.grid_step_deg) {
                return Err(AppError::BadRequest("网格步长需在 [0.1, 30] 度内".into()));
            }
            if !(0.0..=90.0).contains(&params.min_elevation_deg) {
                return Err(AppError::BadRequest("最小仰角需在 [0, 90] 度内".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "environment" => {
            let params: EnvironmentParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("环境参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择卫星".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "propagation_compare" => {
            let params: CompareParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("对比参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择卫星".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "link_budget" => {
            let params: LinkParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("链路参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() || params.ground_station_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择卫星与地面站".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "constellation_analysis" => {
            let params: ConstellationAnalysisParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("星座分析参数无效：{err}")))?;
            if !(0.1..=30.0).contains(&params.grid_step_deg) {
                return Err(AppError::BadRequest("网格步长需在 [0.1, 30] 度内".into()));
            }
            if !(0.0..=90.0).contains(&params.min_elevation_deg) {
                return Err(AppError::BadRequest("最小仰角需在 [0, 90] 度内".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "radar" => {
            let params: RadarParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("雷达参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() || params.ground_station_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择卫星与地面站".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        "report" => {
            let request: ReportRequest = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("报告参数无效：{err}")))?;
            let start = parse_time(&request.start, "start")?;
            let end = parse_time(&request.end, "end")?;
            validate_window(start, end, request.step_seconds)
        }
        "interference" => {
            let params: InterferenceParams = serde_json::from_value(params.clone())
                .map_err(|err| AppError::BadRequest(format!("干扰参数无效：{err}")))?;
            if params.satellite_id.trim().is_empty() || params.ground_station_id.trim().is_empty() {
                return Err(AppError::BadRequest("请选择目标卫星与地面站".into()));
            }
            if params
                .interferer_ids
                .iter()
                .filter(|id| !id.trim().is_empty() && **id != params.satellite_id)
                .count()
                == 0
            {
                return Err(AppError::BadRequest("请至少选择一个干扰源卫星".into()));
            }
            let start = parse_time(&params.start, "start")?;
            let end = parse_time(&params.end, "end")?;
            validate_window(start, end, params.step_seconds)
        }
        _ => Err(AppError::BadRequest(format!("未知任务类型：{kind}"))),
    }
}

fn canonical_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_default()
}

/// 缓存键严格绑定任务类型、参数与场景内容，避免错误复用。
fn compute_cache_key(kind: &str, params: &Value, scenario_data: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(b"|params|");
    hasher.update(canonical_json(params).as_bytes());
    hasher.update(b"|scenario|");
    hasher.update(canonical_json(scenario_data).as_bytes());
    format!("{:x}", hasher.finalize())
}

async fn fetch_job(state: &AppState, user: &AuthUser, id: Uuid) -> Result<JobRow, AppError> {
    let row: Option<JobRow> = sqlx::query_as(&format!(
        "SELECT {JOB_COLUMNS} FROM computation_jobs j
         WHERE j.id = $3 AND {scope}",
        scope = policy::JOB_READ_SCOPE
    ))
    .bind(user.id)
    .bind(user.org_id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(AppError::NotFound("任务不存在"))
}

async fn create_job(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateJobRequest>,
) -> Result<Json<JobView>, AppError> {
    user.require_writer()?;
    let access = policy::scenario_access(&state.pool, &user, payload.scenario_id).await?;
    if !access.can_write {
        return Err(AppError::Forbidden("对该场景只有只读权限，无法创建计算任务"));
    }
    validate_params(&payload.kind, &payload.params)?;
    policy::enforce_job_quota(&state.pool, &user).await?;

    let data: Option<Value> = sqlx::query_scalar("SELECT data FROM scenarios WHERE id = $1")
        .bind(payload.scenario_id)
        .fetch_optional(&state.pool)
        .await?;
    let data = data.ok_or(AppError::NotFound("场景不存在"))?;
    let cache_key = compute_cache_key(&payload.kind, &payload.params, &data);

    // 结果缓存：仅复用有效期内、参数与场景内容完全一致的成功结果。
    let cached: Option<(Uuid, Value)> = sqlx::query_as(
        "SELECT id, result FROM computation_jobs
         WHERE org_id IS NOT DISTINCT FROM $1
           AND cache_key = $2
           AND status = 'succeeded'
           AND result IS NOT NULL
           AND finished_at > now() - make_interval(hours => $3)
         ORDER BY finished_at DESC
         LIMIT 1",
    )
    .bind(user.org_id)
    .bind(&cache_key)
    .bind(state.config.job_cache_hours.max(1) as i32)
    .fetch_optional(&state.pool)
    .await?;

    let job_id = Uuid::new_v4();
    if let Some((source_id, result)) = cached {
        sqlx::query(
            "INSERT INTO computation_jobs
                (id, org_id, created_by, scenario_id, kind, status, params, result, progress,
                 cached, cache_key, cache_source_id, started_at, finished_at)
             VALUES ($1, $2, $3, $4, $5, 'succeeded', $6, $7, 100, TRUE, $8, $9, now(), now())",
        )
        .bind(job_id)
        .bind(user.org_id)
        .bind(user.id)
        .bind(payload.scenario_id)
        .bind(&payload.kind)
        .bind(&payload.params)
        .bind(result)
        .bind(&cache_key)
        .bind(source_id)
        .execute(&state.pool)
        .await?;

        let row = fetch_job(&state, &user, job_id).await?;
        return Ok(Json(row.into_view()));
    }

    sqlx::query(
        "INSERT INTO computation_jobs
            (id, org_id, created_by, scenario_id, kind, status, params, progress, cache_key)
         VALUES ($1, $2, $3, $4, $5, 'queued', $6, 0, $7)",
    )
    .bind(job_id)
    .bind(user.org_id)
    .bind(user.id)
    .bind(payload.scenario_id)
    .bind(&payload.kind)
    .bind(&payload.params)
    .bind(&cache_key)
    .execute(&state.pool)
    .await?;

    let background = state.clone();
    tokio::spawn(async move {
        execute_job(background, job_id).await;
    });

    let row = fetch_job(&state, &user, job_id).await?;
    Ok(Json(row.into_view()))
}

async fn list_jobs(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<JobQuery>,
) -> Result<Json<Vec<JobSummaryView>>, AppError> {
    let limit = query.limit.unwrap_or(100).clamp(1, 200);
    let rows: Vec<JobSummaryRow> = sqlx::query_as(&format!(
        "SELECT {JOB_SUMMARY_COLUMNS} FROM computation_jobs j
         WHERE {scope}
           AND ($3::uuid IS NULL OR j.scenario_id = $3)
           AND ($4::text IS NULL OR j.status = $4)
         ORDER BY j.created_at DESC
         LIMIT $5",
        scope = policy::JOB_READ_SCOPE
    ))
    .bind(user.id)
    .bind(user.org_id)
    .bind(query.scenario_id)
    .bind(query.status.as_deref())
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter().map(JobSummaryRow::into_view).collect(),
    ))
}

async fn get_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, AppError> {
    let row = fetch_job(&state, &user, id).await?;
    Ok(Json(row.into_view()))
}

async fn cancel_job(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<JobView>, AppError> {
    user.require_writer()?;
    let row = fetch_job(&state, &user, id).await?;

    let owner: Option<(Option<Uuid>, Option<Uuid>)> =
        sqlx::query_as("SELECT created_by, org_id FROM computation_jobs WHERE id = $1")
            .bind(id)
            .fetch_optional(&state.pool)
            .await?;
    let (creator, job_org) = owner.ok_or(AppError::NotFound("任务不存在"))?;
    let allowed = creator == Some(user.id)
        || (user.is_admin() && job_org.is_some() && job_org == user.org_id);
    if !allowed {
        return Err(AppError::Forbidden("仅任务创建者或管理员可取消任务"));
    }

    if matches!(row.status.as_str(), "succeeded" | "failed" | "cancelled") {
        return Err(AppError::Conflict("任务已结束，无法取消".into()));
    }

    if row.status == "queued" {
        sqlx::query(
            "UPDATE computation_jobs
             SET status = 'cancelled', cancel_requested = TRUE, cancelled_at = now(), finished_at = now()
             WHERE id = $1 AND status = 'queued'",
        )
        .bind(id)
        .execute(&state.pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE computation_jobs SET cancel_requested = TRUE WHERE id = $1",
        )
        .bind(id)
        .execute(&state.pool)
        .await?;
    }

    policy::audit(
        &state.pool,
        &user,
        actions::JOB_CANCEL,
        "job",
        Some(id),
        json!({ "status": row.status }),
    )
    .await;

    let row = fetch_job(&state, &user, id).await?;
    Ok(Json(row.into_view()))
}

async fn job_csv(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Response, AppError> {
    let row = fetch_job(&state, &user, id).await?;
    if row.status != "succeeded" {
        return Err(AppError::BadRequest("任务尚未完成，无法导出".into()));
    }
    let result = row
        .result
        .ok_or(AppError::BadRequest("任务没有结果".into()))?;

    let (csv, filename) = match row.kind.as_str() {
        "access" => {
            let report: AccessReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("Access 结果解析失败：{err}")))?;
            (access_csv(&report), format!("access-{id}.csv"))
        }
        "coverage" => {
            let report: CoverageReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("覆盖结果解析失败：{err}")))?;
            (coverage_csv(&report), format!("coverage-{id}.csv"))
        }
        "environment" => {
            let report: EnvironmentReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("环境结果解析失败：{err}")))?;
            (environment_csv(&report), format!("environment-{id}.csv"))
        }
        "propagation_compare" => {
            let report: PropagationCompareReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("对比结果解析失败：{err}")))?;
            (compare_csv(&report), format!("propagation-compare-{id}.csv"))
        }
        "link_budget" => {
            let report: LinkBudgetReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("链路结果解析失败：{err}")))?;
            (
                format!("\u{feff}{}", link::link_csv(&report)),
                format!("link-budget-{id}.csv"),
            )
        }
        "constellation_analysis" => {
            let report: ConstellationAnalysisReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("星座分析结果解析失败：{err}")))?;
            (
                format!("\u{feff}{}", constellation_csv(&report)),
                format!("constellation-analysis-{id}.csv"),
            )
        }
        "radar" => {
            let report: RadarReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("雷达结果解析失败：{err}")))?;
            (
                format!("\u{feff}{}", radar::radar_csv(&report)),
                format!("radar-{id}.csv"),
            )
        }
        "interference" => {
            let report: InterferenceReport = serde_json::from_value(result)
                .map_err(|err| AppError::Internal(format!("干扰结果解析失败：{err}")))?;
            (
                format!("\u{feff}{}", link::interference_csv(&report)),
                format!("interference-{id}.csv"),
            )
        }
        _ => return Err(AppError::BadRequest("该任务类型不支持 CSV 导出".into())),
    };

    Ok((
        [
            (
                header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        csv,
    )
        .into_response())
}

async fn job_report(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Query(query): Query<ReportDownloadQuery>,
) -> Result<Response, AppError> {
    let row = fetch_job(&state, &user, id).await?;
    if row.status != "succeeded" {
        return Err(AppError::BadRequest("任务尚未完成，无法导出报告".into()));
    }
    let result = row
        .result
        .ok_or(AppError::BadRequest("任务没有结果".into()))?;
    let document: ReportDocument = serde_json::from_value(result)
        .map_err(|err| AppError::Internal(format!("报告结果解析失败：{err}")))?;

    let format = query.format.unwrap_or_else(|| "html".to_string());
    let (content_type, body, filename) = if format.eq_ignore_ascii_case("markdown")
        || format.eq_ignore_ascii_case("md")
    {
        (
            "text/markdown; charset=utf-8",
            document.markdown,
            format!("kst-report-{id}.md"),
        )
    } else {
        (
            "text/html; charset=utf-8",
            document.html,
            format!("kst-report-{id}.html"),
        )
    };

    Ok((
        [
            (header::CONTENT_TYPE, content_type.to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        body,
    )
        .into_response())
}

fn access_csv(report: &AccessReport) -> String {
    let mut output = String::from(
        "\u{feff}window,satellite,ground_station,constraints,sensor,min_elevation_deg,start_utc,end_utc,duration_seconds,max_elevation_deg\n",
    );
    for window in &report.windows {
        output.push_str(&format!(
            "{},{},{},{},{},{:.2},{},{},{:.1},{:.3}\n",
            window.index,
            csv_field(&report.satellite_name),
            csv_field(&report.ground_station_name),
            csv_field(&window.constraints.join("|")),
            csv_field(report.sensor_name.as_deref().unwrap_or("")),
            report.min_elevation_deg,
            window.start_utc,
            window.end_utc,
            window.duration_seconds,
            window.max_elevation_deg
        ));
    }
    output
}

fn coverage_csv(report: &CoverageReport) -> String {
    let mut output = String::from(
        "\u{feff}latitude_deg,longitude_deg,visible_seconds,access_count,max_elevation_deg,covered\n",
    );
    for point in &report.points {
        output.push_str(&format!(
            "{:.4},{:.4},{:.1},{},{:.3},{}\n",
            point.latitude_deg,
            point.longitude_deg,
            point.visible_seconds,
            point.access_count,
            point.max_elevation_deg,
            if point.covered { 1 } else { 0 }
        ));
    }
    output
}

fn environment_csv(report: &EnvironmentReport) -> String {
    let mut output = String::from(
        "\u{feff}t_seconds,altitude_km,corrected_altitude_km,density_kg_m3,temperature_k,drag_accel_m_s2,cumulative_delta_v_m_s\n",
    );
    for sample in &report.samples {
        output.push_str(&format!(
            "{:.1},{:.3},{:.3},{:.6e},{:.2},{:.6e},{:.3}\n",
            sample.t_seconds,
            sample.altitude_km,
            sample.corrected_altitude_km,
            sample.density_kg_m3,
            sample.temperature_k,
            sample.drag_accel_m_s2,
            sample.cumulative_delta_v_m_s
        ));
    }
    output
}

fn compare_csv(report: &PropagationCompareReport) -> String {
    let mut output = String::from(
        "\u{feff}t_seconds,sgp4_longitude_deg,sgp4_latitude_deg,sgp4_altitude_km,numerical_longitude_deg,numerical_latitude_deg,numerical_altitude_km,position_difference_km,velocity_difference_km_s\n",
    );
    for sample in &report.samples {
        output.push_str(&format!(
            "{:.1},{:.6},{:.6},{:.3},{:.6},{:.6},{:.3},{:.6},{:.9}\n",
            sample.t_seconds,
            sample.sgp4_longitude_deg,
            sample.sgp4_latitude_deg,
            sample.sgp4_altitude_km,
            sample.numerical_longitude_deg,
            sample.numerical_latitude_deg,
            sample.numerical_altitude_km,
            sample.position_difference_km,
            sample.velocity_difference_km_s
        ));
    }
    output
}

fn constellation_csv(report: &ConstellationAnalysisReport) -> String {
    let mut output = String::from(
        "latitude_deg,longitude_deg,access_count,visible_seconds,mean_gap_seconds,max_gap_seconds,max_elevation_deg,covered\n",
    );
    for point in &report.revisit_points {
        output.push_str(&format!(
            "{:.4},{:.4},{},{:.1},{:.1},{:.1},{:.3},{}\n",
            point.latitude_deg,
            point.longitude_deg,
            point.access_count,
            point.visible_seconds,
            point.mean_gap_seconds,
            point.max_gap_seconds,
            point.max_elevation_deg,
            if point.covered { 1 } else { 0 }
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

async fn set_progress(state: &AppState, job_id: Uuid, value: i32) {
    let _ = sqlx::query(
        "UPDATE computation_jobs SET progress = $2 WHERE id = $1 AND status = 'running'",
    )
    .bind(job_id)
    .bind(value)
    .execute(&state.pool)
    .await;
}

async fn run_job(state: AppState, job_id: Uuid) -> Result<(), String> {
    // 仅由 queued 状态抢占执行，避免重复执行或执行已取消任务。
    let claimed = sqlx::query(
        "UPDATE computation_jobs
         SET status = 'running', started_at = now(), progress = 10
         WHERE id = $1 AND status = 'queued' AND cancel_requested = FALSE",
    )
    .bind(job_id)
    .execute(&state.pool)
    .await
    .map_err(|err| err.to_string())?;
    if claimed.rows_affected() == 0 {
        let _ = sqlx::query(
            "UPDATE computation_jobs
             SET status = 'cancelled', cancelled_at = COALESCE(cancelled_at, now()),
                 finished_at = COALESCE(finished_at, now())
             WHERE id = $1 AND status = 'queued'",
        )
        .bind(job_id)
        .execute(&state.pool)
        .await;
        return Ok(());
    }

    let row: Option<JobRow> = sqlx::query_as(&format!(
        "SELECT {JOB_COLUMNS} FROM computation_jobs WHERE id = $1"
    ))
    .bind(job_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|err| err.to_string())?;
    let row = row.ok_or_else(|| "任务不存在".to_string())?;

    set_progress(&state, job_id, 25).await;

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

    set_progress(&state, job_id, 85).await;

    // 计算完成后若用户已请求取消，则丢弃结果。
    let cancel_requested: Option<bool> =
        sqlx::query_scalar("SELECT cancel_requested FROM computation_jobs WHERE id = $1")
            .bind(job_id)
            .fetch_optional(&state.pool)
            .await
            .map_err(|err| err.to_string())?;
    if cancel_requested == Some(true) {
        sqlx::query(
            "UPDATE computation_jobs
             SET status = 'cancelled', result = NULL, progress = 100,
                 cancelled_at = now(), finished_at = now()
             WHERE id = $1",
        )
        .bind(job_id)
        .execute(&state.pool)
        .await
        .map_err(|err| err.to_string())?;
        return Ok(());
    }

    sqlx::query(
        "UPDATE computation_jobs
         SET status = 'succeeded', result = $2, error = NULL, progress = 100, finished_at = now()
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
                &params.options(),
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "coverage" => {
            let params: CoverageParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("覆盖参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = orbit::compute_coverage(
                scenario,
                params.satellite_ids.as_deref(),
                start,
                end,
                params.step_seconds,
                params.region,
                params.grid_step_deg,
                params.min_elevation_deg,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "environment" => {
            let params: EnvironmentParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("环境参数无效：{err}"))?;
            let satellite = scenario
                .satellite(&params.satellite_id)
                .ok_or_else(|| format!("场景中不存在卫星 {}", params.satellite_id))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = environment::analyze_environment(
                satellite,
                start,
                end,
                params.step_seconds,
                &params.options,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "propagation_compare" => {
            let params: CompareParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("对比参数无效：{err}"))?;
            let satellite = scenario
                .satellite(&params.satellite_id)
                .ok_or_else(|| format!("场景中不存在卫星 {}", params.satellite_id))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report =
                orbit::compare_propagation(satellite, start, end, params.step_seconds)?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "link_budget" => {
            let params: LinkParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("链路参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = link::compute_link_budget(
                scenario,
                &params.satellite_id,
                &params.ground_station_id,
                start,
                end,
                params.step_seconds,
                &params.options,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "constellation_analysis" => {
            let params: ConstellationAnalysisParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("星座分析参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = orbit::compute_constellation_analysis(
                scenario,
                params.satellite_ids.as_deref(),
                params.station_ids.as_deref(),
                start,
                end,
                params.step_seconds,
                params.region,
                params.grid_step_deg,
                params.min_elevation_deg,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "radar" => {
            let params: RadarParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("雷达参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = radar::compute_radar(
                scenario,
                &params.satellite_id,
                &params.ground_station_id,
                start,
                end,
                params.step_seconds,
                &params.options,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        "report" => {
            let request: ReportRequest = serde_json::from_value(params.clone())
                .map_err(|err| format!("报告参数无效：{err}"))?;
            let document = report::generate(scenario, &request)?;
            serde_json::to_value(document).map_err(|err| err.to_string())
        }
        "interference" => {
            let params: InterferenceParams = serde_json::from_value(params.clone())
                .map_err(|err| format!("干扰参数无效：{err}"))?;
            let start = parse_time(&params.start, "start").map_err(|err| err.to_string())?;
            let end = parse_time(&params.end, "end").map_err(|err| err.to_string())?;
            let report = link::compute_interference(
                scenario,
                &params.satellite_id,
                &params.ground_station_id,
                &params.interferer_ids,
                start,
                end,
                params.step_seconds,
                &params.options,
                params.threshold_ci_db,
            )?;
            serde_json::to_value(report).map_err(|err| err.to_string())
        }
        other => Err(format!("未知任务类型：{other}")),
    }
}
