use std::sync::Arc;

use sqlx::PgPool;

use crate::{auth::AuthService, space::SpaceContext};

#[derive(Clone)]
pub struct AppState {
    pub pool: PgPool,
    pub auth: AuthService,
    pub space: Arc<SpaceContext>,
}
