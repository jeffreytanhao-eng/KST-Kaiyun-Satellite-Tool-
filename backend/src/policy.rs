//! 访问控制、审计与配额。
//!
//! 所有权限判断均在服务端执行：场景读取遵循“创建者 / 同组织 / 显式共享 / 管理员”，
//! 写入还需要“创建者 / 管理员 / 可编辑共享”。任务与 AI 结果复用同一套场景可见性。

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{auth::AuthUser, error::AppError};

#[derive(Debug, Clone)]
pub struct ScenarioAccess {
    pub scenario_id: Uuid,
    pub org_id: Option<Uuid>,
    pub owner_id: Option<Uuid>,
    pub is_owner: bool,
    pub can_write: bool,
    /// owner / admin / write / read
    pub permission: &'static str,
}

impl ScenarioAccess {
    pub fn require_write(&self) -> Result<(), AppError> {
        if self.can_write {
            Ok(())
        } else {
            Err(AppError::Forbidden("对该场景只有只读权限"))
        }
    }

    pub fn require_owner(&self) -> Result<(), AppError> {
        if self.is_owner || self.permission == "admin" {
            Ok(())
        } else {
            Err(AppError::Forbidden("仅场景所有者或管理员可执行该操作"))
        }
    }
}

/// 计算用户对指定场景的权限；不可见时返回 404，避免泄露跨组织资源存在性。
pub async fn scenario_access(
    pool: &PgPool,
    user: &AuthUser,
    scenario_id: Uuid,
) -> Result<ScenarioAccess, AppError> {
    let row: Option<(Option<Uuid>, Option<Uuid>, Option<String>)> = sqlx::query_as(
        "SELECT s.org_id, s.created_by,
                (SELECT sh.permission FROM scenario_shares sh
                  WHERE sh.scenario_id = s.id AND sh.user_id = $2)
         FROM scenarios s
         WHERE s.id = $1",
    )
    .bind(scenario_id)
    .bind(user.id)
    .fetch_optional(pool)
    .await?;

    let (org_id, owner_id, share_permission) =
        row.ok_or(AppError::NotFound("场景不存在"))?;

    let is_owner = owner_id == Some(user.id);
    let same_org = org_id.is_some() && org_id == user.org_id;
    let is_admin = user.is_admin() && same_org;
    let can_read = is_owner || same_org || is_admin || share_permission.is_some();
    if !can_read {
        return Err(AppError::NotFound("场景不存在"));
    }

    let can_write = is_owner || is_admin || share_permission.as_deref() == Some("write");
    let permission = if is_owner {
        "owner"
    } else if is_admin {
        "admin"
    } else if can_write {
        "write"
    } else {
        "read"
    };

    Ok(ScenarioAccess {
        scenario_id,
        org_id,
        owner_id,
        is_owner,
        can_write,
        permission,
    })
}

/// 任务可见性 SQL 片段（固定使用 $1 = 用户 ID，$2 = 组织 ID）。
pub const JOB_READ_SCOPE: &str = "(
    j.created_by = $1
    OR j.scenario_id IN (
        SELECT s.id FROM scenarios s
        WHERE s.created_by = $1
           OR (s.org_id IS NOT NULL AND s.org_id = $2)
           OR EXISTS (
               SELECT 1 FROM scenario_shares sh
               WHERE sh.scenario_id = s.id AND sh.user_id = $1
           )
    )
)";

/// 写入审计日志；失败时仅告警，不影响主流程。
pub async fn audit(
    pool: &PgPool,
    user: &AuthUser,
    action: &str,
    resource_type: &str,
    resource_id: Option<Uuid>,
    details: Value,
) {
    let result = sqlx::query(
        "INSERT INTO audit_logs (id, org_id, user_id, action, resource_type, resource_id, details)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(user.org_id)
    .bind(user.id)
    .bind(action)
    .bind(resource_type)
    .bind(resource_id)
    .bind(details)
    .execute(pool)
    .await;

    if let Err(err) = result {
        tracing::warn!(error = %err, action, "审计日志写入失败");
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_concurrent_jobs: i64,
    pub max_daily_jobs: i64,
    pub max_ai_daily: i64,
}

/// 用户配额：个人覆盖优先，其次组织配额，最后回退默认值。
pub async fn effective_limits(pool: &PgPool, user: &AuthUser) -> Result<Limits, AppError> {
    let row: Option<(Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>, Option<i64>)> =
        sqlx::query_as(
            "SELECT o.max_concurrent_jobs::bigint, o.max_daily_jobs::bigint,
                    o.max_ai_daily::bigint,
                    u.max_concurrent_jobs::bigint, u.max_daily_jobs::bigint,
                    u.max_ai_daily::bigint
             FROM users u
             LEFT JOIN organizations o ON o.id = u.org_id
             WHERE u.id = $1",
        )
        .bind(user.id)
        .fetch_optional(pool)
        .await?;

    let (org_concurrent, org_daily, org_ai, user_concurrent, user_daily, user_ai) =
        row.unwrap_or((None, None, None, None, None, None));

    Ok(Limits {
        max_concurrent_jobs: user_concurrent.or(org_concurrent).unwrap_or(5),
        max_daily_jobs: user_daily.or(org_daily).unwrap_or(500),
        max_ai_daily: user_ai.or(org_ai).unwrap_or(200),
    })
}

/// 创建计算任务前的配额校验（并发 + 每日总量）。
pub async fn enforce_job_quota(pool: &PgPool, user: &AuthUser) -> Result<(), AppError> {
    let limits = effective_limits(pool, user).await?;

    let active: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE created_by = $1 AND status IN ('queued', 'running')",
    )
    .bind(user.id)
    .fetch_one(pool)
    .await?;
    if active >= limits.max_concurrent_jobs {
        return Err(AppError::TooManyRequests(format!(
            "并发计算任务已达上限（{}），请等待现有任务完成或取消任务",
            limits.max_concurrent_jobs
        )));
    }

    let daily: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM computation_jobs
         WHERE created_by = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(user.id)
    .fetch_one(pool)
    .await?;
    if daily >= limits.max_daily_jobs {
        return Err(AppError::TooManyRequests(format!(
            "今日计算任务已达上限（{}），请明日再试",
            limits.max_daily_jobs
        )));
    }

    Ok(())
}

/// AI 调用前的每日配额校验。
pub async fn enforce_ai_quota(pool: &PgPool, user: &AuthUser) -> Result<(), AppError> {
    let limits = effective_limits(pool, user).await?;
    let daily: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM ai_requests
         WHERE created_by = $1 AND created_at >= date_trunc('day', now())",
    )
    .bind(user.id)
    .fetch_one(pool)
    .await?;
    if daily >= limits.max_ai_daily {
        return Err(AppError::TooManyRequests(format!(
            "今日 AI 调用已达上限（{}），请明日再试",
            limits.max_ai_daily
        )));
    }
    Ok(())
}

/// 供审计详情使用的轻量 JSON 构造。
pub fn details(pairs: &[(&str, Value)]) -> Value {
    let mut map = serde_json::Map::new();
    for (key, value) in pairs {
        map.insert((*key).to_string(), value.clone());
    }
    Value::Object(map)
}

/// 常用审计动作名。
pub mod actions {
    pub const SCENARIO_SHARE_UPSERT: &str = "scenario.share.upsert";
    pub const SCENARIO_SHARE_DELETE: &str = "scenario.share.delete";
    pub const SCENARIO_DELETE: &str = "scenario.delete";
    pub const SCENARIO_COMMENT: &str = "scenario.comment.create";
    pub const JOB_CANCEL: &str = "job.cancel";
    pub const ROLE_UPDATE: &str = "org.role.update";
    pub const QUOTA_UPDATE: &str = "org.quota.update";
    pub const INTERPRETATION_SAVE: &str = "ai.interpretation.save";
}
