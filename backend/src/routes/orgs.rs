//! 组织管理：角色、配额使用情况与审计日志。

use axum::{
    extract::{Path, Query, State},
    routing::{get, patch, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::AppError,
    policy::{self, actions},
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/usage", get(usage))
        .route("/members", get(list_members))
        .route("/members/{id}", patch(update_member_role))
        .route("/members/{id}/quotas", patch(update_member_quotas))
        .route("/quotas", put(update_org_quotas))
        .route("/audit", get(list_audit))
}

fn require_org(user: &AuthUser) -> Result<Uuid, AppError> {
    user.org_id
        .ok_or(AppError::BadRequest("当前账号未加入组织".into()))
}

#[derive(Debug, Serialize)]
struct LimitsView {
    max_concurrent_jobs: i64,
    max_daily_jobs: i64,
    max_ai_daily: i64,
}

#[derive(Debug, Serialize)]
struct UsageView {
    org_id: Uuid,
    org_name: String,
    role: String,
    limits: LimitsView,
    active_jobs: i64,
    daily_jobs: i64,
    daily_ai: i64,
    org_active_jobs: i64,
    org_daily_jobs: i64,
    job_retention_hours: i64,
    job_cache_hours: i64,
    ai_enabled: bool,
}

async fn usage(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<UsageView>, AppError> {
    let org_id = require_org(&user)?;
    let limits = policy::effective_limits(&state.pool, &user).await?;

    let org_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM organizations WHERE id = $1")
            .bind(org_id)
            .fetch_optional(&state.pool)
            .await?;

    let active_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE created_by = $1 AND status IN ('queued', 'running')",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    let daily_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE created_by = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    let daily_ai: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_requests
         WHERE created_by = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;

    let org_active_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE org_id = $1 AND status IN ('queued', 'running')",
    )
    .bind(org_id)
    .fetch_one(&state.pool)
    .await?;

    let org_daily_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE org_id = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(org_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(UsageView {
        org_id,
        org_name: org_name.unwrap_or_else(|| "Organization".to_string()),
        role: user.role.clone(),
        limits: LimitsView {
            max_concurrent_jobs: limits.max_concurrent_jobs,
            max_daily_jobs: limits.max_daily_jobs,
            max_ai_daily: limits.max_ai_daily,
        },
        active_jobs,
        daily_jobs,
        daily_ai,
        org_active_jobs,
        org_daily_jobs,
        job_retention_hours: state.config.job_retention_hours,
        job_cache_hours: state.config.job_cache_hours,
        ai_enabled: state.ai.enabled(),
    }))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct MemberView {
    id: Uuid,
    username: String,
    email: Option<String>,
    role: String,
    max_concurrent_jobs: Option<i32>,
    max_daily_jobs: Option<i32>,
    max_ai_daily: Option<i32>,
    created_at: DateTime<Utc>,
    active_jobs: i64,
}

async fn list_members(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<MemberView>>, AppError> {
    let org_id = require_org(&user)?;
    let rows: Vec<MemberView> = sqlx::query_as(
        "SELECT u.id, u.username, u.email, u.role,
                u.max_concurrent_jobs, u.max_daily_jobs, u.max_ai_daily, u.created_at,
                (SELECT count(*) FROM computation_jobs j
                  WHERE j.created_by = u.id AND j.status IN ('queued', 'running')) AS active_jobs
         FROM users u
         WHERE u.org_id = $1
         ORDER BY u.created_at ASC",
    )
    .bind(org_id)
    .fetch_all(&state.pool)
    .await?;
    Ok(Json(rows))
}

#[derive(Debug, Deserialize)]
struct RoleUpdateRequest {
    role: String,
}

fn validate_role(role: &str) -> Result<&'static str, AppError> {
    match role.trim() {
        "admin" => Ok("admin"),
        "analyst" => Ok("analyst"),
        "viewer" => Ok("viewer"),
        _ => Err(AppError::BadRequest(
            "角色只能是 admin、analyst 或 viewer".into(),
        )),
    }
}

async fn update_member_role(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(payload): Json<RoleUpdateRequest>,
) -> Result<Json<MemberView>, AppError> {
    user.require_admin()?;
    let org_id = require_org(&user)?;
    let role = validate_role(&payload.role)?;

    let target: Option<(String,)> =
        sqlx::query_as("SELECT role FROM users WHERE id = $1 AND org_id = $2")
            .bind(target_id)
            .bind(org_id)
            .fetch_optional(&state.pool)
            .await?;
    let (current_role,) = target.ok_or(AppError::NotFound("组织成员不存在"))?;

    if target_id == user.id && role != "admin" {
        return Err(AppError::BadRequest("不能移除自己的管理员角色".into()));
    }
    if current_role == "admin" && role != "admin" {
        let admin_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM users WHERE org_id = $1 AND role = 'admin'")
                .bind(org_id)
                .fetch_one(&state.pool)
                .await?;
        if admin_count <= 1 {
            return Err(AppError::BadRequest("组织至少需要保留一名管理员".into()));
        }
    }

    sqlx::query("UPDATE users SET role = $1 WHERE id = $2 AND org_id = $3")
        .bind(role)
        .bind(target_id)
        .bind(org_id)
        .execute(&state.pool)
        .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::ROLE_UPDATE,
        "user",
        Some(target_id),
        json!({ "from": current_role, "to": role }),
    )
    .await;

    load_member(&state, org_id, target_id).await.map(Json)
}

async fn load_member(
    state: &AppState,
    org_id: Uuid,
    target_id: Uuid,
) -> Result<MemberView, AppError> {
    let row: Option<MemberView> = sqlx::query_as(
        "SELECT u.id, u.username, u.email, u.role,
                u.max_concurrent_jobs, u.max_daily_jobs, u.max_ai_daily, u.created_at,
                (SELECT count(*) FROM computation_jobs j
                  WHERE j.created_by = u.id AND j.status IN ('queued', 'running')) AS active_jobs
         FROM users u
         WHERE u.id = $1 AND u.org_id = $2",
    )
    .bind(target_id)
    .bind(org_id)
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(AppError::NotFound("组织成员不存在"))
}

#[derive(Debug, Deserialize)]
struct QuotaUpdateRequest {
    #[serde(default)]
    max_concurrent_jobs: Option<i64>,
    #[serde(default)]
    max_daily_jobs: Option<i64>,
    #[serde(default)]
    max_ai_daily: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct MemberQuotaRequest {
    #[serde(default)]
    max_concurrent_jobs: Option<i64>,
    #[serde(default)]
    max_daily_jobs: Option<i64>,
    #[serde(default)]
    max_ai_daily: Option<i64>,
    /// 清除个人覆盖，恢复使用组织默认配额。
    #[serde(default)]
    reset: bool,
}

fn validate_limit(value: Option<i64>, label: &str) -> Result<Option<i64>, AppError> {
    if let Some(limit) = value {
        if limit < 1 || limit > 100_000 {
            return Err(AppError::BadRequest(format!(
                "{label} 需在 1 到 100000 之间"
            )));
        }
        Ok(Some(limit))
    } else {
        Ok(None)
    }
}

async fn update_org_quotas(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<QuotaUpdateRequest>,
) -> Result<Json<Value>, AppError> {
    user.require_admin()?;
    let org_id = require_org(&user)?;
    let concurrent = validate_limit(payload.max_concurrent_jobs, "并发任务上限")?;
    let daily = validate_limit(payload.max_daily_jobs, "每日任务上限")?;
    let ai = validate_limit(payload.max_ai_daily, "每日 AI 上限")?;

    sqlx::query(
        "UPDATE organizations
         SET max_concurrent_jobs = COALESCE($1, max_concurrent_jobs),
             max_daily_jobs = COALESCE($2, max_daily_jobs),
             max_ai_daily = COALESCE($3, max_ai_daily)
         WHERE id = $4",
    )
    .bind(concurrent)
    .bind(daily)
    .bind(ai)
    .bind(org_id)
    .execute(&state.pool)
    .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::QUOTA_UPDATE,
        "organization",
        Some(org_id),
        json!({
            "max_concurrent_jobs": concurrent,
            "max_daily_jobs": daily,
            "max_ai_daily": ai
        }),
    )
    .await;

    Ok(Json(json!({ "ok": true })))
}

async fn update_member_quotas(
    State(state): State<AppState>,
    user: AuthUser,
    Path(target_id): Path<Uuid>,
    Json(payload): Json<MemberQuotaRequest>,
) -> Result<Json<MemberView>, AppError> {
    user.require_admin()?;
    let org_id = require_org(&user)?;

    let exists: Option<(Uuid,)> =
        sqlx::query_as("SELECT id FROM users WHERE id = $1 AND org_id = $2")
            .bind(target_id)
            .bind(org_id)
            .fetch_optional(&state.pool)
            .await?;
    if exists.is_none() {
        return Err(AppError::NotFound("组织成员不存在"));
    }

    if payload.reset {
        sqlx::query(
            "UPDATE users
             SET max_concurrent_jobs = NULL, max_daily_jobs = NULL, max_ai_daily = NULL
             WHERE id = $1 AND org_id = $2",
        )
        .bind(target_id)
        .bind(org_id)
        .execute(&state.pool)
        .await?;
    } else {
        let concurrent = validate_limit(payload.max_concurrent_jobs, "并发任务上限")?;
        let daily = validate_limit(payload.max_daily_jobs, "每日任务上限")?;
        let ai = validate_limit(payload.max_ai_daily, "每日 AI 上限")?;
        sqlx::query(
            "UPDATE users
             SET max_concurrent_jobs = COALESCE($1, max_concurrent_jobs),
                 max_daily_jobs = COALESCE($2, max_daily_jobs),
                 max_ai_daily = COALESCE($3, max_ai_daily)
             WHERE id = $4 AND org_id = $5",
        )
        .bind(concurrent)
        .bind(daily)
        .bind(ai)
        .bind(target_id)
        .bind(org_id)
        .execute(&state.pool)
        .await?;
    }

    policy::audit(
        &state.pool,
        &user,
        actions::QUOTA_UPDATE,
        "user",
        Some(target_id),
        json!({ "reset": payload.reset }),
    )
    .await;

    load_member(&state, org_id, target_id).await.map(Json)
}

#[derive(Debug, Deserialize)]
struct AuditQuery {
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct AuditView {
    id: Uuid,
    user_id: Option<Uuid>,
    username: Option<String>,
    action: String,
    resource_type: String,
    resource_id: Option<Uuid>,
    details: Value,
    created_at: DateTime<Utc>,
}

async fn list_audit(
    State(state): State<AppState>,
    user: AuthUser,
    Query(query): Query<AuditQuery>,
) -> Result<Json<Vec<AuditView>>, AppError> {
    user.require_admin()?;
    let org_id = require_org(&user)?;
    let limit = query.limit.unwrap_or(100).clamp(1, 500);

    let rows: Vec<AuditView> = sqlx::query_as(
        "SELECT a.id, a.user_id, u.username, a.action, a.resource_type, a.resource_id,
                a.details, a.created_at
         FROM audit_logs a
         LEFT JOIN users u ON u.id = a.user_id
         WHERE a.org_id = $1
         ORDER BY a.created_at DESC
         LIMIT $2",
    )
    .bind(org_id)
    .bind(limit)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}
