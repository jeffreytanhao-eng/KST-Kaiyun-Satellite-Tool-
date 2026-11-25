use argon2::{
    password_hash::{rand_core::OsRng, PasswordHasher, SaltString},
    Argon2,
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::error::AppError;

pub async fn connect(url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(url)
        .await
}

pub async fn migrate(pool: &PgPool) -> Result<(), sqlx::migrate::MigrateError> {
    sqlx::migrate!("./migrations").run(pool).await
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|err| AppError::Internal(format!("密码哈希失败: {err}")))
}

/// 库中没有任何用户时，用环境变量提供的账号创建初始管理员。
pub async fn ensure_admin(pool: &PgPool, username: &str, password: &str) -> Result<(), AppError> {
    let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(pool)
        .await?;
    if user_count > 0 {
        return Ok(());
    }

    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name) VALUES ($1, $2)")
        .bind(org_id)
        .bind("默认组织")
        .execute(pool)
        .await?;

    let password_hash = hash_password(password)?;
    sqlx::query(
        "INSERT INTO users (id, org_id, username, password_hash) VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::new_v4())
    .bind(org_id)
    .bind(username)
    .bind(password_hash)
    .execute(pool)
    .await?;

    tracing::info!(%username, "已创建初始管理员账号");
    Ok(())
}
