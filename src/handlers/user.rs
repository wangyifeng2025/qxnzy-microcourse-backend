use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::enums::UserRole;
use crate::models::pagination::{PageQuery, PagedList};
use crate::models::user::{
    AdminResetPasswordRequest, ChangePasswordRequest, CreateUser, RegisterRequest, UpdateUser,
    UserProfile,
};
use crate::repositories::user as user_repo;

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

fn forbidden(msg: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, msg.to_string())
}

fn ensure_self_or_admin(auth: &AuthContext, target_user_id: Uuid) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin || auth.user_id == target_user_id {
        return Ok(());
    }

    Err(forbidden("仅本人或管理员可访问该用户资源"))
}

/// GET /api/users?page_size=20&cursor_created_at=...&cursor_id=...
pub async fn list_users(
    State(pool): State<PgPool>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<UserProfile>> {
    let result = user_repo::find_all(&pool, &query).await.map_err(internal_error)?;
    Ok(Json(result))
}

/// GET /api/users/:id
pub async fn get_user(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<UserProfile> {
    ensure_self_or_admin(&auth, id)?;

    let user = user_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("用户不存在"))?;
    Ok(Json(user))
}

/// POST /api/auth/register — 公开注册，角色固定为 Student
pub async fn register_user(
    State(pool): State<PgPool>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<UserProfile>), (StatusCode, String)> {
    if payload.username.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "用户名不能为空".to_string()));
    }
    if payload.password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "密码长度不能少于 6 位".to_string()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(internal_error)?
        .to_string();

    let user = user_repo::create(
        &pool,
        &payload.username,
        payload.email.as_deref(),
        &password_hash,
        &UserRole::Student,
        payload.real_name.as_deref(),
    )
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.code().as_deref() == Some("23505") {
                return (StatusCode::CONFLICT, "用户名或邮箱已存在".to_string());
            }
        }
        internal_error(e)
    })?;

    Ok((StatusCode::CREATED, Json(user)))
}

/// POST /api/users — 仅管理员，可指定任意角色
pub async fn create_user(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateUser>,
) -> Result<(StatusCode, Json<UserProfile>), (StatusCode, String)> {
    if payload.username.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "用户名不能为空".to_string()));
    }
    if payload.password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "密码长度不能少于 6 位".to_string()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.password.as_bytes(), &salt)
        .map_err(internal_error)?
        .to_string();

    let user = user_repo::create(
        &pool,
        &payload.username,
        payload.email.as_deref(),
        &password_hash,
        &payload.role,
        payload.real_name.as_deref(),
    )
    .await
    .map_err(|e| {
        if let sqlx::Error::Database(ref db_err) = e {
            if db_err.code().as_deref() == Some("23505") {
                return (StatusCode::CONFLICT, "用户名或邮箱已存在".to_string());
            }
        }
        internal_error(e)
    })?;

    Ok((StatusCode::CREATED, Json(user)))
}

/// PUT /api/users/:id
pub async fn update_user(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateUser>,
) -> AppResult<UserProfile> {
    ensure_self_or_admin(&auth, id)?;

    if payload.is_active.is_some() && auth.role != UserRole::Admin {
        return Err(forbidden("仅管理员可修改账号启用状态"));
    }

    if payload.role.is_some() && auth.role != UserRole::Admin {
        return Err(forbidden("仅管理员可修改用户角色"));
    }

    // major_id：学生可修改本人，管理员可修改任意用户；教师/管理员角色设置 major_id 无意义但不拦截
    // ensure_self_or_admin 已确保非本人 + 非管理员不能走到此处，无需额外判断

    let user = user_repo::update(&pool, id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("用户不存在"))?;
    Ok(Json(user))
}

/// DELETE /api/users/:id
pub async fn delete_user(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let rows = user_repo::delete(&pool, id)
        .await
        .map_err(internal_error)?;

    if rows == 0 {
        return Err(not_found("用户不存在"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/users/:id/reset-password — 仅管理员
/// 重置后 password_reset_required = true，前端应提示用户自行修改密码
pub async fn admin_reset_password(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(payload): Json<AdminResetPasswordRequest>,
) -> AppResult<UserProfile> {
    if payload.new_password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "密码长度不能少于 6 位".to_string()));
    }

    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(internal_error)?
        .to_string();

    let user = user_repo::reset_password(&pool, id, &password_hash)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("用户不存在"))?;

    Ok(Json(user))
}

/// PUT /api/users/:id/change-password — 仅本人
/// 需要提供旧密码验证身份；成功后 password_reset_required 清除为 false
pub async fn change_password(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ChangePasswordRequest>,
) -> AppResult<UserProfile> {
    if auth.user_id != id {
        return Err(forbidden("仅本人可修改自己的密码"));
    }

    if payload.new_password.len() < 6 {
        return Err((StatusCode::BAD_REQUEST, "新密码长度不能少于 6 位".to_string()));
    }

    // 查出旧密码哈希，验证旧密码正确性
    let user = user_repo::find_by_id_with_hash(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("用户不存在"))?;

    let parsed_hash = PasswordHash::new(&user.password_hash)
        .map_err(|_| internal_error("密码哈希格式异常"))?;

    Argon2::default()
        .verify_password(payload.old_password.as_bytes(), &parsed_hash)
        .map_err(|_| (StatusCode::UNAUTHORIZED, "旧密码不正确".to_string()))?;

    let salt = SaltString::generate(&mut OsRng);
    let new_hash = Argon2::default()
        .hash_password(payload.new_password.as_bytes(), &salt)
        .map_err(internal_error)?
        .to_string();

    let updated = user_repo::change_password(&pool, id, &new_hash)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("用户不存在"))?;

    Ok(Json(updated))
}
