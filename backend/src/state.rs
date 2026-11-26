use std::sync::Arc;

use sqlx::PgPool;

use crate::{
    ai::AiClient, auth::AuthService, config::Config, rate_limit::RateLimiter, space::SpaceContext,
};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthService,
    pub space: Arc<SpaceContext>,
    pub ai: Arc<AiClient>,
    pub config: Arc<Config>,
    pub limiter: Arc<RateLimiter>,
}
