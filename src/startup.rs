use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use sqlx::PgPool;

use crate::models::enums::UserRole;
use crate::repositories::user as user_repo;

/// 应用启动时确保管理员账号存在。
///
/// 从环境变量读取配置：
/// - `ADMIN_USERNAME`  管理员用户名，默认 `admin`
/// - `ADMIN_PASSWORD`  管理员密码（必须显式设置，否则使用不安全的默认值并打印警告）
/// - `ADMIN_EMAIL`     管理员邮箱，可选
/// - `ADMIN_REAL_NAME` 管理员真实姓名，默认 `系统管理员`
///
/// 若该用户名已存在则跳过创建（不覆盖已有密码）。
pub async fn seed_admin(pool: &PgPool) {
    let username =
        std::env::var("ADMIN_USERNAME").unwrap_or_else(|_| "admin".to_string());
    let password = match std::env::var("ADMIN_PASSWORD") {
        Ok(p) if !p.trim().is_empty() => p,
        _ => {
            tracing::warn!(
                "ADMIN_PASSWORD 未设置，使用默认密码 \"admin123\"，请在生产环境中修改！"
            );
            "admin123".to_string()
        }
    };
    let email = std::env::var("ADMIN_EMAIL").ok();
    let real_name =
        std::env::var("ADMIN_REAL_NAME").unwrap_or_else(|_| "系统管理员".to_string());

    match user_repo::find_by_username(pool, &username).await {
        Ok(Some(_)) => {
            tracing::info!(username = %username, "管理员账号已存在，跳过初始化");
            return;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!(error = %e, "查询管理员账号失败，跳过初始化");
            return;
        }
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = match Argon2::default()
        .hash_password(password.as_bytes(), &salt)
    {
        Ok(h) => h.to_string(),
        Err(e) => {
            tracing::error!(error = %e, "管理员密码哈希失败，跳过初始化");
            return;
        }
    };

    match user_repo::create(
        pool,
        &username,
        email.as_deref(),
        &password_hash,
        &UserRole::Admin,
        Some(&real_name),
    )
    .await
    {
        Ok(admin) => {
            tracing::info!(
                id = %admin.id,
                username = %admin.username,
                "管理员账号初始化成功"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, "管理员账号创建失败");
        }
    }
}
