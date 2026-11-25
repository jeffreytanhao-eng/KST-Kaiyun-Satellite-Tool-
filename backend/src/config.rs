use std::env;

/// 服务运行配置，全部通过环境变量注入。
#[derive(Debug, Clone)]
pub struct Config {
    /// HTTP 监听地址，默认 0.0.0.0:8080。
    pub http_addr: String,
    pub database_url: String,
    pub jwt_secret: String,
    pub jwt_ttl_hours: i64,
    pub admin_username: String,
    pub admin_password: String,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        let database_url =
            env::var("DATABASE_URL").map_err(|_| "缺少环境变量 DATABASE_URL".to_string())?;
        let jwt_secret =
            env::var("KST_JWT_SECRET").map_err(|_| "缺少环境变量 KST_JWT_SECRET".to_string())?;
        if jwt_secret.len() < 32 {
            return Err("KST_JWT_SECRET 长度至少 32 个字符".to_string());
        }

        let jwt_ttl_hours = env::var("KST_JWT_TTL_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(12);

        Ok(Self {
            http_addr: env::var("KST_HTTP_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string()),
            database_url,
            jwt_secret,
            jwt_ttl_hours,
            admin_username: env::var("KST_ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string()),
            admin_password: env::var("KST_ADMIN_PASSWORD").unwrap_or_default(),
        })
    }
}
