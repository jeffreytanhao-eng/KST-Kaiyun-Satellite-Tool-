use axum::{
    extract::{Path, State},
    routing::{get, post, put},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::AppError,
    policy::{self, actions},
    scenario::ScenarioData,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_scenarios).post(create_scenario))
        .route("/import", post(import_scenario))
        .route(
            "/{id}",
            get(get_scenario)
                .put(update_scenario)
                .delete(delete_scenario),
        )
        .route("/{id}/shares", get(list_shares).post(upsert_share))
        .route(
            "/{id}/shares/{user_id}",
            put(upsert_share_for_user).delete(delete_share),
        )
        .route("/{id}/comments", get(list_comments).post(create_comment))
}

#[derive(Debug, sqlx::FromRow)]
struct ScenarioRow {
    id: Uuid,
    name: String,
    description: String,
    data: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    created_by: Option<Uuid>,
    org_id: Option<Uuid>,
    share_permission: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScenarioView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub data: ScenarioData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub owner_id: Option<Uuid>,
    pub permission: String,
    pub can_edit: bool,
}

#[derive(Debug, Serialize)]
struct ScenarioSummary {
    id: Uuid,
    name: String,
    description: String,
    satellites: usize,
    ground_stations: usize,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    owner_id: Option<Uuid>,
    permission: String,
    can_edit: bool,
}

fn permission_of(row: &ScenarioRow, user: &AuthUser) -> (String, bool) {
    let is_owner = row.created_by == Some(user.id);
    let same_org = row.org_id.is_some() && row.org_id == user.org_id;
    let is_admin = user.is_admin() && same_org;
    let can_write = is_owner || is_admin || row.share_permission.as_deref() == Some("write");
    let label = if is_owner {
        "owner"
    } else if is_admin {
        "admin"
    } else if can_write {
        "write"
    } else {
        "read"
    };
    (label.to_string(), can_write)
}

impl ScenarioRow {
    fn into_view(self, user: &AuthUser) -> Result<ScenarioView, AppError> {
        let (permission, can_edit) = permission_of(&self, user);
        let data: ScenarioData = serde_json::from_value(self.data)
            .map_err(|err| AppError::Internal(format!("场景数据损坏：{err}")))?;
        Ok(ScenarioView {
            id: self.id,
            name: self.name,
            description: self.description,
            data,
            created_at: self.created_at,
            updated_at: self.updated_at,
            owner_id: self.created_by,
            permission,
            can_edit,
        })
    }

    fn into_summary(self, user: &AuthUser) -> Result<ScenarioSummary, AppError> {
        let view = self.into_view(user)?;
        Ok(ScenarioSummary {
            id: view.id,
            name: view.name,
            description: view.description,
            satellites: view.data.satellites.len(),
            ground_stations: view.data.ground_stations.len(),
            created_at: view.created_at,
            updated_at: view.updated_at,
            owner_id: view.owner_id,
            permission: view.permission,
            can_edit: view.can_edit,
        })
    }
}

#[derive(Debug, Deserialize)]
struct CreateScenarioRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    data: ScenarioData,
}

#[derive(Debug, Deserialize)]
struct UpdateScenarioRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    data: ScenarioData,
}

#[derive(Debug, Deserialize)]
struct ShareRequest {
    username: String,
    #[serde(default = "default_share_permission")]
    permission: String,
}

#[derive(Debug, Deserialize)]
struct ShareUpdateRequest {
    #[serde(default = "default_share_permission")]
    permission: String,
}

#[derive(Debug, Deserialize)]
struct CommentRequest {
    body: String,
}

fn default_share_permission() -> String {
    "read".to_string()
}

const SELECT_SCENARIO: &str = "SELECT s.id, s.name, s.description, s.data, s.created_at, s.updated_at,
        s.created_by, s.org_id,
        (SELECT sh.permission FROM scenario_shares sh
          WHERE sh.scenario_id = s.id AND sh.user_id = $1) AS share_permission
     FROM scenarios s";

fn validate_name(name: &str) -> Result<String, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("场景名称不能为空".into()));
    }
    if trimmed.chars().count() > 120 {
        return Err(AppError::BadRequest("场景名称不能超过 120 个字符".into()));
    }
    Ok(trimmed.to_string())
}

fn validate_permission(permission: &str) -> Result<&'static str, AppError> {
    match permission.trim() {
        "read" => Ok("read"),
        "write" => Ok("write"),
        _ => Err(AppError::BadRequest("共享权限只能是 read 或 write".into())),
    }
}

async fn load_row(
    state: &AppState,
    user: &AuthUser,
    id: Uuid,
) -> Result<ScenarioRow, AppError> {
    let row: Option<ScenarioRow> = sqlx::query_as(&format!(
        "{SELECT_SCENARIO} WHERE s.id = $2"
    ))
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(AppError::NotFound("场景不存在"))
}

async fn list_scenarios(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ScenarioSummary>>, AppError> {
    let rows: Vec<ScenarioRow> = sqlx::query_as(&format!(
        "{SELECT_SCENARIO}
         WHERE s.created_by = $1
            OR (s.org_id IS NOT NULL AND s.org_id = $2)
            OR EXISTS (SELECT 1 FROM scenario_shares sh
                        WHERE sh.scenario_id = s.id AND sh.user_id = $1)
         ORDER BY s.updated_at DESC"
    ))
    .bind(user.id)
    .bind(user.org_id)
    .fetch_all(&state.pool)
    .await?;

    let mut summaries = Vec::with_capacity(rows.len());
    for row in rows {
        summaries.push(row.into_summary(&user)?);
    }
    Ok(Json(summaries))
}

async fn create_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateScenarioRequest>,
) -> Result<Json<ScenarioView>, AppError> {
    user.require_writer()?;
    let name = validate_name(&payload.name)?;
    payload.data.validate()?;

    let data =
        serde_json::to_value(&payload.data).map_err(|err| AppError::Internal(err.to_string()))?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO scenarios (id, org_id, created_by, name, description, data)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(id)
    .bind(user.org_id)
    .bind(user.id)
    .bind(name)
    .bind(payload.description.trim())
    .bind(data)
    .execute(&state.pool)
    .await?;

    let row = load_row(&state, &user, id).await?;
    Ok(Json(row.into_view(&user)?))
}

async fn import_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateScenarioRequest>,
) -> Result<Json<ScenarioView>, AppError> {
    create_scenario(State(state), user, Json(payload)).await
}

async fn get_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<ScenarioView>, AppError> {
    policy::scenario_access(&state.pool, &user, id).await?;
    let row = load_row(&state, &user, id).await?;
    Ok(Json(row.into_view(&user)?))
}

async fn update_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateScenarioRequest>,
) -> Result<Json<ScenarioView>, AppError> {
    user.require_writer()?;
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_write()?;

    let name = validate_name(&payload.name)?;
    payload.data.validate()?;
    let data =
        serde_json::to_value(&payload.data).map_err(|err| AppError::Internal(err.to_string()))?;

    sqlx::query(
        "UPDATE scenarios
         SET name = $1, description = $2, data = $3, updated_at = now()
         WHERE id = $4",
    )
    .bind(name)
    .bind(payload.description.trim())
    .bind(data)
    .bind(id)
    .execute(&state.pool)
    .await?;

    let row = load_row(&state, &user, id).await?;
    Ok(Json(row.into_view(&user)?))
}

async fn delete_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    user.require_writer()?;
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_owner()?;

    sqlx::query("DELETE FROM scenarios WHERE id = $1")
        .bind(id)
        .execute(&state.pool)
        .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::SCENARIO_DELETE,
        "scenario",
        Some(id),
        json!({}),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct ShareView {
    id: Uuid,
    user_id: Uuid,
    username: String,
    permission: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

async fn list_shares(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<ShareView>>, AppError> {
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_owner()?;

    let rows: Vec<ShareView> = sqlx::query_as(
        "SELECT sh.id, sh.user_id, u.username, sh.permission, sh.created_at, sh.updated_at
         FROM scenario_shares sh
         JOIN users u ON u.id = sh.user_id
         WHERE sh.scenario_id = $1
         ORDER BY sh.created_at DESC",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

async fn upsert_share(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<ShareRequest>,
) -> Result<Json<ShareView>, AppError> {
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_owner()?;
    let permission = validate_permission(&payload.permission)?;
    let username = payload.username.trim();
    if username.is_empty() {
        return Err(AppError::BadRequest("请填写共享对象用户名".into()));
    }

    let target: Option<(Uuid,)> = sqlx::query_as("SELECT id FROM users WHERE username = $1")
        .bind(username)
        .fetch_optional(&state.pool)
        .await?;
    let (target_id,) = target.ok_or(AppError::NotFound("共享目标用户不存在"))?;
    if target_id == user.id {
        return Err(AppError::BadRequest("不能共享给自己".into()));
    }

    sqlx::query(
        "INSERT INTO scenario_shares (id, scenario_id, user_id, permission, created_by)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (scenario_id, user_id)
         DO UPDATE SET permission = EXCLUDED.permission, updated_at = now()",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .bind(target_id)
    .bind(permission)
    .bind(user.id)
    .execute(&state.pool)
    .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::SCENARIO_SHARE_UPSERT,
        "scenario",
        Some(id),
        json!({ "target_user": target_id, "permission": permission }),
    )
    .await;

    load_share(&state, id, target_id).await.map(Json)
}

async fn upsert_share_for_user(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<ShareUpdateRequest>,
) -> Result<Json<ShareView>, AppError> {
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_owner()?;
    let permission = validate_permission(&payload.permission)?;

    let affected = sqlx::query(
        "UPDATE scenario_shares SET permission = $1, updated_at = now()
         WHERE scenario_id = $2 AND user_id = $3",
    )
    .bind(permission)
    .bind(id)
    .bind(target_id)
    .execute(&state.pool)
    .await?;
    if affected.rows_affected() == 0 {
        return Err(AppError::NotFound("共享记录不存在"));
    }

    policy::audit(
        &state.pool,
        &user,
        actions::SCENARIO_SHARE_UPSERT,
        "scenario",
        Some(id),
        json!({ "target_user": target_id, "permission": permission }),
    )
    .await;

    load_share(&state, id, target_id).await.map(Json)
}

async fn delete_share(
    State(state): State<AppState>,
    user: AuthUser,
    Path((id, target_id)): Path<(Uuid, Uuid)>,
) -> Result<axum::http::StatusCode, AppError> {
    let access = policy::scenario_access(&state.pool, &user, id).await?;
    access.require_owner()?;

    let affected = sqlx::query("DELETE FROM scenario_shares WHERE scenario_id = $1 AND user_id = $2")
        .bind(id)
        .bind(target_id)
        .execute(&state.pool)
        .await?;
    if affected.rows_affected() == 0 {
        return Err(AppError::NotFound("共享记录不存在"));
    }

    policy::audit(
        &state.pool,
        &user,
        actions::SCENARIO_SHARE_DELETE,
        "scenario",
        Some(id),
        json!({ "target_user": target_id }),
    )
    .await;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

async fn load_share(
    state: &AppState,
    scenario_id: Uuid,
    target_id: Uuid,
) -> Result<ShareView, AppError> {
    let row: Option<ShareView> = sqlx::query_as(
        "SELECT sh.id, sh.user_id, u.username, sh.permission, sh.created_at, sh.updated_at
         FROM scenario_shares sh
         JOIN users u ON u.id = sh.user_id
         WHERE sh.scenario_id = $1 AND sh.user_id = $2",
    )
    .bind(scenario_id)
    .bind(target_id)
    .fetch_optional(&state.pool)
    .await?;
    row.ok_or(AppError::NotFound("共享记录不存在"))
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct CommentView {
    id: Uuid,
    user_id: Option<Uuid>,
    username: Option<String>,
    body: String,
    created_at: DateTime<Utc>,
}

async fn list_comments(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<CommentView>>, AppError> {
    policy::scenario_access(&state.pool, &user, id).await?;

    let rows: Vec<CommentView> = sqlx::query_as(
        "SELECT c.id, c.user_id, u.username, c.body, c.created_at
         FROM scenario_comments c
         LEFT JOIN users u ON u.id = c.user_id
         WHERE c.scenario_id = $1
         ORDER BY c.created_at ASC
         LIMIT 200",
    )
    .bind(id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

async fn create_comment(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<CommentRequest>,
) -> Result<Json<CommentView>, AppError> {
    policy::scenario_access(&state.pool, &user, id).await?;
    let body = payload.body.trim();
    if body.is_empty() {
        return Err(AppError::BadRequest("评论内容不能为空".into()));
    }
    if body.chars().count() > 2000 {
        return Err(AppError::BadRequest("评论不能超过 2000 个字符".into()));
    }

    let comment_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO scenario_comments (id, scenario_id, user_id, body)
         VALUES ($1, $2, $3, $4)",
    )
    .bind(comment_id)
    .bind(id)
    .bind(user.id)
    .bind(body)
    .execute(&state.pool)
    .await?;

    policy::audit(
        &state.pool,
        &user,
        actions::SCENARIO_COMMENT,
        "scenario",
        Some(id),
        json!({ "comment_id": comment_id }),
    )
    .await;

    let row: CommentView = sqlx::query_as(
        "SELECT c.id, c.user_id, u.username, c.body, c.created_at
         FROM scenario_comments c
         LEFT JOIN users u ON u.id = c.user_id
         WHERE c.id = $1",
    )
    .bind(comment_id)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row))
}
