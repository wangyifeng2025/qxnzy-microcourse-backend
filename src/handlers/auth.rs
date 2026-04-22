use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{repositories::user as user_repo, utils::jwt::encode_token};

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginUserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub role: crate::models::enums::UserRole,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    /// 管理员重置密码后为 true，前端应强制引导用户修改密码
    pub password_reset_required: bool,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: &'static str,
    pub expires_at: usize,
    pub user: LoginUserInfo,
}

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn unauthorized(msg: &str) -> (StatusCode, String) {
    (StatusCode::UNAUTHORIZED, msg.to_string())
}

fn bad_request(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

/// POST /api/auth/login
pub async fn login(
    State(pool): State<PgPool>,
    Json(payload): Json<LoginRequest>,
) -> AppResult<LoginResponse> {
    let password = payload.password.trim();
    if password.is_empty() {
        return Err(bad_request("密码不能为空"));
    }

    let user = if let Some(username) = payload.username.as_deref().map(str::trim) {
        if username.is_empty() {
            return Err(bad_request("用户名不能为空"));
        }
        user_repo::find_by_username(&pool, username)
            .await
            .map_err(internal_error)?
    } else if let Some(email) = payload.email.as_deref().map(str::trim) {
        if email.is_empty() {
            return Err(bad_request("邮箱不能为空"));
        }
        user_repo::find_by_email(&pool, email)
            .await
            .map_err(internal_error)?
    } else {
        return Err(bad_request("请提供 username 或 email"));
    };

    let user = user.ok_or_else(|| unauthorized("账号或密码错误"))?;
    if !user.is_active {
        return Err(unauthorized("账号已被禁用"));
    }

    let parsed_hash =
        PasswordHash::new(&user.password_hash).map_err(|_| unauthorized("账号或密码错误"))?;
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| unauthorized("账号或密码错误"))?;

    let (token, expires_at) = encode_token(user.id, &user.role).map_err(internal_error)?;

    Ok(Json(LoginResponse {
        token,
        token_type: "Bearer",
        expires_at,
        user: LoginUserInfo {
            id: user.id,
            username: user.username,
            email: user.email,
            role: user.role,
            real_name: user.real_name,
            avatar_url: user.avatar_url,
            password_reset_required: user.password_reset_required,
        },
    }))
}
