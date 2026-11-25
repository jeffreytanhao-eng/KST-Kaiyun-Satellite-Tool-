mod ai;
mod auth;
mod config;
mod db;
mod environment;
mod error;
mod orbit;
mod routes;
mod scenario;
mod sensor;
mod space;
mod spaceenv;
mod state;

use std::net::SocketAddr;

use axum::Router;
use tower_http::trace::TraceLayer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "kst_server=info,tower_http=info".into()),
        )
        .init();

    let config = config::Config::from_env().map_err(std::io::Error::other)?;

    let pool = db::connect(&config.database_url).await?;
    db::migrate(&pool).await?;

    if !config.admin_password.is_empty() {
        db::ensure_admin(&pool, &config.admin_username, &config.admin_password).await?;
    } else {
        tracing::warn!("未设置 KST_ADMIN_PASSWORD，跳过初始管理员创建");
    }

    let auth = auth::AuthService::new(config.jwt_secret.clone(), config.jwt_ttl_hours);
    let space = space::SpaceContext::new()?;
    let ai = ai::AiClient::new(&config);
    if ai.enabled() {
        tracing::info!("AI 功能已启用");
    } else {
        tracing::warn!("AI 功能未启用（缺少 KST_AI_API_KEY / KST_AI_MODEL 或显式关闭）");
    }
    let state = state::AppState {
        pool,
        auth,
        space: std::sync::Arc::new(space),
        ai: std::sync::Arc::new(ai),
    };

    let app = Router::new()
        .nest("/api/v1", routes::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = config.http_addr.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "KST backend listening");
    axum::serve(listener, app).await?;

    Ok(())
}
