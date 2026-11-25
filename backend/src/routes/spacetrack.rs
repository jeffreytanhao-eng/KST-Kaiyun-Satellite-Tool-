//! Space-Track TLE 导入接口。
//!
//! 通过账号登录 Space-Track，再查询 `basicspacedata/gp` 获取最新 TLE。
//! 由于 Space-Track 使用 Cookie 会话，这里手动保存登录返回的 Cookie。
//! 速率限制与账号注册要求见 Space-Track 官方说明；本接口仅提供手动刷新。

use axum::{extract::State, routing::post, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{auth::AuthUser, error::AppError, state::AppState};

pub fn router() -> Router<AppState> {
    Router::new().route("/tle", post(fetch_tle))
}

#[derive(Debug, Deserialize)]
struct TleRequest {
    #[serde(default)]
    norad_ids: Vec<u64>,
}

#[derive(Debug, Serialize)]
struct SpaceTrackTle {
    norad_id: u64,
    name: String,
    tle_line1: String,
    tle_line2: String,
    epoch: Option<String>,
}

#[derive(Debug, Serialize)]
struct TleResponse {
    count: usize,
    items: Vec<SpaceTrackTle>,
}

/// 对查询参数做最小百分号编码。
fn encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char)
            }
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

fn text_field(record: &Value, key: &str) -> Option<String> {
    record.get(key).and_then(|value| match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    })
}

async fn fetch_tle(
    State(state): State<AppState>,
    _user: AuthUser,
    Json(request): Json<TleRequest>,
) -> Result<Json<TleResponse>, AppError> {
    let username = state.config.space_track_username.trim();
    let password = state.config.space_track_password.trim();
    if username.is_empty() || password.is_empty() {
        return Err(AppError::BadRequest(
            "未配置 Space-Track 账号，请在 .env 中设置 KST_SPACETRACK_USERNAME 与 KST_SPACETRACK_PASSWORD"
                .into(),
        ));
    }

    let mut ids: Vec<u64> = request
        .norad_ids
        .into_iter()
        .filter(|id| *id > 0)
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if ids.is_empty() {
        return Err(AppError::BadRequest("请至少提供一个 NORAD 编号".into()));
    }
    if ids.len() > 50 {
        return Err(AppError::BadRequest("单次最多查询 50 个 NORAD 编号".into()));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|err| AppError::Internal(format!("HTTP 客户端创建失败：{err}")))?;

    let login_body = format!(
        "identity={}&password={}",
        encode(username),
        encode(password)
    );
    let login = client
        .post("https://www.space-track.org/ajaxauth/login")
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(login_body)
        .send()
        .await
        .map_err(|err| AppError::Internal(format!("Space-Track 登录请求失败：{err}")))?;

    if !login.status().is_success() {
        return Err(AppError::Internal(format!(
            "Space-Track 登录失败：HTTP {}",
            login.status()
        )));
    }

    let cookies: Vec<String> = login
        .headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.split(';').next())
        .map(str::to_string)
        .collect();
    if cookies.is_empty() {
        return Err(AppError::Internal(
            "Space-Track 登录未返回会话 Cookie，请检查账号是否有效".into(),
        ));
    }

    let id_list = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let query_url = format!(
        "https://www.space-track.org/basicspacedata/query/class/gp/NORAD_CAT_ID/{id_list}/orderby/NORAD_CAT_ID/format/json"
    );

    let response = client
        .get(query_url)
        .header(reqwest::header::COOKIE, cookies.join("; "))
        .header(reqwest::header::REFERER, "https://www.space-track.org/")
        .send()
        .await
        .map_err(|err| AppError::Internal(format!("Space-Track 查询失败：{err}")))?;

    if !response.status().is_success() {
        return Err(AppError::Internal(format!(
            "Space-Track 查询失败：HTTP {}",
            response.status()
        )));
    }

    let payload: Value = response
        .json()
        .await
        .map_err(|err| AppError::Internal(format!("Space-Track 响应解析失败：{err}")))?;
    let records = payload
        .as_array()
        .ok_or_else(|| AppError::Internal("Space-Track 响应格式不是数组".into()))?;

    let mut items = Vec::with_capacity(records.len());
    for record in records {
        let Some(norad_id) = text_field(record, "NORAD_CAT_ID").and_then(|id| id.parse().ok())
        else {
            continue;
        };
        let Some(name) = text_field(record, "OBJECT_NAME") else {
            continue;
        };
        let Some(tle_line1) = text_field(record, "TLE_LINE1") else {
            continue;
        };
        let Some(tle_line2) = text_field(record, "TLE_LINE2") else {
            continue;
        };
        if tle_line1.len() < 69 || tle_line2.len() < 69 {
            continue;
        }
        items.push(SpaceTrackTle {
            norad_id,
            name,
            tle_line1,
            tle_line2,
            epoch: text_field(record, "EPOCH"),
        });
    }

    if items.is_empty() {
        return Err(AppError::BadRequest(
            "Space-Track 未返回有效的 TLE，请检查 NORAD 编号".into(),
        ));
    }

    Ok(Json(TleResponse {
        count: items.len(),
        items,
    }))
}
