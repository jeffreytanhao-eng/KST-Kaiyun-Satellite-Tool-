use std::{sync::Arc, time::Duration};

use axum::{
    extract::{FromRequestParts, State},
    http::{header::AUTHORIZATION, request::Parts},
    routing::{get, post},
    Json, Router,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use uuid::Uuid;

use crate::{error::AppError, state::AppState};

type HmacSha256 = Hmac<Sha256>;

/// 无状态的 HS256 JWT 签发与校验。
#[derive(Clone)]
pub struct AuthService {
    secret: Arc<Vec<u8>>,
    ttl: Duration,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,
    pub username: String,
    pub org_id: Option<Uuid>,
    #[serde(default = "default_role")]
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

fn default_role() -> String {
    "analyst".to_string()
}

/// 通过 Bearer Token 解析出的当前用户。
#[derive(Clone)]
pub struct AuthUser {
    pub id: Uuid,
    pub org_id: Option<Uuid>,
    pub role: String,
}

impl AuthUser {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }

    pub fn is_viewer(&self) -> bool {
        self.role == "viewer"
    }

    /// 只读角色不能创建计算任务、AI 生成或修改场景。
    pub fn require_writer(&self) -> Result<(), AppError> {
        if self.is_viewer() {
            Err(AppError::Forbidden("当前角色为只读，无写入权限"))
        } else {
            Ok(())
        }
    }

    pub fn require_admin(&self) -> Result<(), AppError> {
        if self.is_admin() {
            Ok(())
        } else {
            Err(AppError::Forbidden("需要组织管理员权限"))
        }
    }
}

impl AuthService {
    pub fn new(secret: String, ttl_hours: i64) -> Self {
        Self {
            secret: Arc::new(secret.into_bytes()),
            ttl: Duration::from_secs(ttl_hours.max(1) as u64 * 3600),
        }
    }

    pub fn issue(
        &self,
        user_id: Uuid,
        username: &str,
        org_id: Option<Uuid>,
        role: &str,
    ) -> Result<(String, DateTime<Utc>), AppError> {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::from_std(self.ttl).unwrap_or_default();
        let claims = Claims {
            sub: user_id,
            username: username.to_string(),
            org_id,
            role: role.to_string(),
            iat: now.timestamp(),
            exp: expires_at.timestamp(),
        };

        let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&claims).map_err(|err| AppError::Internal(err.to_string()))?,
        );
        let signing_input = format!("{header}.{payload}");

        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|err| AppError::Internal(err.to_string()))?;
        mac.update(signing_input.as_bytes());
        let signature = URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes());

        Ok((format!("{signing_input}.{signature}"), expires_at))
    }

    pub fn verify(&self, token: &str) -> Result<Claims, AppError> {
        let mut segments = token.split('.');
        let (Some(header), Some(payload), Some(signature)) =
            (segments.next(), segments.next(), segments.next())
        else {
            return Err(AppError::Unauthorized("Token 格式无效"));
        };
        if segments.next().is_some() {
            return Err(AppError::Unauthorized("Token 格式无效"));
        }

        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| AppError::Unauthorized("Token 签名无效"))?;
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|err| AppError::Internal(err.to_string()))?;
        mac.update(format!("{header}.{payload}").as_bytes());
        mac.verify_slice(&signature)
            .map_err(|_| AppError::Unauthorized("Token 签名无效"))?;

        let raw = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| AppError::Unauthorized("Token 载荷无效"))?;
        let claims: Claims = serde_json::from_slice(&raw)
            .map_err(|_| AppError::Unauthorized("Token 载荷无效"))?;

        if claims.exp <= Utc::now().timestamp() {
            return Err(AppError::Unauthorized("登录已过期，请重新登录"));
        }
        Ok(claims)
    }
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|value| value.to_str().ok())
            .ok_or(AppError::Unauthorized("缺少认证信息"))?;
        let token = raw
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized("认证格式错误"))?;

        let claims = state.auth.verify(token)?;
        // 角色与组织以数据库为准，确保管理员修改角色后立即生效。
        let row: Option<(Option<Uuid>, String)> =
            sqlx::query_as("SELECT org_id, role FROM users WHERE id = $1")
                .bind(claims.sub)
                .fetch_optional(&state.pool)
                .await?;
        let (org_id, role) = row.ok_or(AppError::Unauthorized("用户不存在，请重新登录"))?;
        Ok(AuthUser {
            id: claims.sub,
            org_id,
            role,
        })
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/me", get(me))
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Serialize, sqlx::FromRow)]
struct UserView {
    id: Uuid,
    username: String,
    email: Option<String>,
    org_id: Option<Uuid>,
    role: String,
}

#[derive(Serialize)]
struct LoginResponse {
    token: String,
    expires_at: DateTime<Utc>,
    user: UserView,
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: Uuid,
    username: String,
    email: Option<String>,
    org_id: Option<Uuid>,
    role: String,
    password_hash: String,
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let user: Option<UserRow> = sqlx::query_as(
        "SELECT id, username, email, org_id, role, password_hash FROM users WHERE username = $1",
    )
    .bind(&payload.username)
    .fetch_optional(&state.pool)
    .await?;

    let Some(user) = user else {
        return Err(AppError::Unauthorized("用户名或密码错误"));
    };

    let password = payload.password;
    let stored_hash = user.password_hash.clone();
    let password_ok = tokio::task::spawn_blocking(move || {
        use argon2::{password_hash::PasswordVerifier, Argon2};
        let Ok(parsed) = argon2::PasswordHash::new(&stored_hash) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
    .await
    .map_err(|err| AppError::Internal(err.to_string()))?;

    if !password_ok {
        return Err(AppError::Unauthorized("用户名或密码错误"));
    }

    let (token, expires_at) = state
        .auth
        .issue(user.id, &user.username, user.org_id, &user.role)?;

    Ok(Json(LoginResponse {
        token,
        expires_at,
        user: UserView {
            id: user.id,
            username: user.username,
            email: user.email,
            org_id: user.org_id,
            role: user.role,
        },
    }))
}

async fn me(State(state): State<AppState>, auth: AuthUser) -> Result<Json<UserView>, AppError> {
    let user: Option<UserView> =
        sqlx::query_as("SELECT id, username, email, org_id, role FROM users WHERE id = $1")
            .bind(auth.id)
            .fetch_optional(&state.pool)
            .await?;

    user.map(Json)
        .ok_or(AppError::Unauthorized("用户不存在，请重新登录"))
}
