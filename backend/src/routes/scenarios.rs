use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::AuthUser,
    error::AppError,
    scenario::ScenarioData,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_scenarios).post(create_scenario))
        .route("/import", post(import_scenario))
        .route("/{id}", get(get_scenario).put(update_scenario).delete(delete_scenario))
}

#[derive(Debug, sqlx::FromRow)]
struct ScenarioRow {
    id: Uuid,
    name: String,
    description: String,
    data: serde_json::Value,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct ScenarioView {
    pub id: Uuid,
    pub name: String,
    pub description: String,
    pub data: ScenarioData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
}

impl ScenarioRow {
    fn into_view(self) -> Result<ScenarioView, AppError> {
        let data: ScenarioData = serde_json::from_value(self.data)
            .map_err(|err| AppError::Internal(format!("场景数据损坏：{err}")))?;
        Ok(ScenarioView {
            id: self.id,
            name: self.name,
            description: self.description,
            data,
            created_at: self.created_at,
            updated_at: self.updated_at,
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

const SELECT_SCENARIO: &str = "SELECT id, name, description, data, created_at, updated_at FROM scenarios";
const SCOPE: &str =
    "((org_id IS NOT NULL AND org_id = $1) OR (org_id IS NULL AND created_by = $2))";

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

async fn list_scenarios(
    State(state): State<AppState>,
    user: AuthUser,
) -> Result<Json<Vec<ScenarioSummary>>, AppError> {
    let rows: Vec<ScenarioRow> = sqlx::query_as(&format!(
        "{SELECT_SCENARIO} WHERE {SCOPE} ORDER BY updated_at DESC"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut summaries = Vec::with_capacity(rows.len());
    for row in rows {
        let view = row.into_view()?;
        summaries.push(ScenarioSummary {
            id: view.id,
            name: view.name,
            description: view.description,
            satellites: view.data.satellites.len(),
            ground_stations: view.data.ground_stations.len(),
            created_at: view.created_at,
            updated_at: view.updated_at,
        });
    }

    Ok(Json(summaries))
}

async fn create_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Json(payload): Json<CreateScenarioRequest>,
) -> Result<Json<ScenarioView>, AppError> {
    let name = validate_name(&payload.name)?;
    payload.data.validate()?;

    let data = serde_json::to_value(&payload.data)
        .map_err(|err| AppError::Internal(err.to_string()))?;

    let row: ScenarioRow = sqlx::query_as(
        "INSERT INTO scenarios (id, org_id, created_by, name, description, data)
         VALUES ($1, $2, $3, $4, $5, $6)
         RETURNING id, name, description, data, created_at, updated_at",
    )
    .bind(Uuid::new_v4())
    .bind(user.org_id)
    .bind(user.id)
    .bind(name)
    .bind(payload.description.trim())
    .bind(data)
    .fetch_one(&state.pool)
    .await?;

    Ok(Json(row.into_view()?))
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
    let row: Option<ScenarioRow> = sqlx::query_as(&format!(
        "{SELECT_SCENARIO} WHERE id = $3 AND {SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let row = row.ok_or(AppError::NotFound("场景不存在"))?;
    Ok(Json(row.into_view()?))
}

async fn update_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateScenarioRequest>,
) -> Result<Json<ScenarioView>, AppError> {
    let name = validate_name(&payload.name)?;
    payload.data.validate()?;

    let data = serde_json::to_value(&payload.data)
        .map_err(|err| AppError::Internal(err.to_string()))?;

    let row: Option<ScenarioRow> = sqlx::query_as(&format!(
        "UPDATE scenarios
         SET name = $3, description = $4, data = $5, updated_at = now()
         WHERE id = $6 AND {SCOPE}
         RETURNING id, name, description, data, created_at, updated_at"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(name)
    .bind(payload.description.trim())
    .bind(data)
    .bind(id)
    .fetch_optional(&state.pool)
    .await?;

    let row = row.ok_or(AppError::NotFound("场景不存在"))?;
    Ok(Json(row.into_view()?))
}

async fn delete_scenario(
    State(state): State<AppState>,
    user: AuthUser,
    Path(id): Path<Uuid>,
) -> Result<axum::http::StatusCode, AppError> {
    let result = sqlx::query(&format!(
        "DELETE FROM scenarios WHERE id = $3 AND {SCOPE}"
    ))
    .bind(user.org_id)
    .bind(user.id)
    .bind(id)
    .execute(&state.pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound("场景不存在"));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}
