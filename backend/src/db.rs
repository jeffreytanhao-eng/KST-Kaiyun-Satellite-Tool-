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
        "INSERT INTO users (id, org_id, username, password_hash, role)
         VALUES ($1, $2, $3, $4, 'admin')",
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

/// 周期性清理过期任务结果（按时间保留策略）。
pub fn spawn_job_cleanup(pool: PgPool, retention_hours: i64, interval_minutes: u64) {
    let retention_hours = retention_hours.max(1);
    let interval_minutes = interval_minutes.max(1);
    tokio::spawn(async move {
        let mut ticker =
            tokio::time::interval(std::time::Duration::from_secs(interval_minutes * 60));
        ticker.tick().await;
        loop {
            ticker.tick().await;
            let result = sqlx::query(&format!(
                "DELETE FROM computation_jobs
                 WHERE status IN ('succeeded', 'failed', 'cancelled')
                   AND finished_at IS NOT NULL
                   AND finished_at < now() - interval '{} hours'",
                retention_hours
            ))
            .execute(&pool)
            .await;
            match result {
                Ok(done) => {
                    if done.rows_affected() > 0 {
                        tracing::info!(
                            removed = done.rows_affected(),
                            retention_hours,
                            "已清理过期任务结果"
                        );
                    }
                }
                Err(err) => tracing::warn!(error = %err, "任务结果清理失败"),
            }
        }
    });
}
