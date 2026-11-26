//! 轻量级接口限流。
//!
//! 以 Authorization 头（无鉴权时退化为匿名）为键，采用固定窗口计数，避免
//! 自动化脚本或异常客户端无保护地占用服务资源。

use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

use crate::state::AppState;

struct Bucket {
    window_started: Instant,
    count: u32,
}

pub struct RateLimiter {
    limit: u32,
    window: Duration,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new(limit: u32, window_seconds: u64) -> Self {
        Self {
            limit,
            window: Duration::from_secs(window_seconds),
            buckets: Mutex::new(HashMap::new()),
        }
    }

    fn allow(&self, key: &str) -> bool {
        let mut buckets = match self.buckets.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if buckets.len() > 10_000 {
            let window = self.window;
            buckets.retain(|_, bucket| bucket.window_started.elapsed() < window);
        }
        let now = Instant::now();
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            window_started: now,
            count: 0,
        });
        if now.duration_since(bucket.window_started) >= self.window {
            bucket.window_started = now;
            bucket.count = 0;
        }
        if bucket.count >= self.limit {
            return false;
        }
        bucket.count += 1;
        true
    }
}

/// 限流中间件：返回 429 与统一错误结构。
pub async fn limit(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("anonymous")
        .to_string();
    if !state.limiter.allow(&key) {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({ "error": { "code": "rate_limited", "message": "请求过于频繁，请稍后再试" } })),
        )
            .into_response();
    }
    next.run(request).await
}
