use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    enums::UserRole,
    pagination::{PageQuery, PagedList},
    user::{UpdateUser, User, UserProfile},
};

/// SELECT 字段（含 major_id），供 query_as::<_, UserProfile> 拼接
const PROFILE_COLS: &str =
    "id, username, email, role, real_name, avatar_url, is_active, major_id, password_reset_required, created_at, updated_at";

/// SELECT 字段（含密码哈希 + major_id），供 query_as::<_, User> 拼接
const USER_COLS: &str =
    "id, username, email, password_hash, role, real_name, avatar_url, is_active, major_id, password_reset_required, created_at, updated_at";

pub async fn find_all(pool: &PgPool, query: &PageQuery) -> Result<PagedList<UserProfile>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items: Vec<UserProfile> = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_created_at), Some(cursor_id)) => {
            sqlx::query_as::<_, UserProfile>(&format!(
                "SELECT {} FROM users WHERE (created_at, id) < ($1, $2) ORDER BY created_at DESC, id DESC LIMIT $3",
                PROFILE_COLS
            ))
            .bind(cursor_created_at)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as::<_, UserProfile>(&format!(
                "SELECT {} FROM users ORDER BY created_at DESC, id DESC LIMIT $1",
                PROFILE_COLS
            ))
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
    };

    let has_more = items.len() as i64 > page_size;
    if has_more {
        items.truncate(page_size as usize);
    }
    let (next_cursor_created_at, next_cursor_id) = if has_more {
        items
            .last()
            .map(|u| (Some(u.created_at), Some(u.id)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(PagedList {
        page_size,
        has_more,
        next_cursor_created_at,
        next_cursor_id,
        items,
    })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserProfile>, sqlx::Error> {
    sqlx::query_as::<_, UserProfile>(&format!(
        "SELECT {} FROM users WHERE id = $1",
        PROFILE_COLS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

/// 含密码哈希的查询（供修改密码时验证旧密码）
pub async fn find_by_id_with_hash(pool: &PgPool, id: Uuid) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {} FROM users WHERE id = $1",
        USER_COLS
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_username(pool: &PgPool, username: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {} FROM users WHERE username = $1",
        USER_COLS
    ))
    .bind(username)
    .fetch_optional(pool)
    .await
}

pub async fn find_by_email(pool: &PgPool, email: &str) -> Result<Option<User>, sqlx::Error> {
    sqlx::query_as::<_, User>(&format!(
        "SELECT {} FROM users WHERE email = $1",
        USER_COLS
    ))
    .bind(email)
    .fetch_optional(pool)
    .await
}

/// 用于鉴权：用户不存在视为未启用
pub async fn is_user_active(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let row: Option<bool> = sqlx::query_scalar(r#"SELECT is_active FROM users WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.unwrap_or(false))
}

pub async fn create(
    pool: &PgPool,
    username: &str,
    email: Option<&str>,
    password_hash: &str,
    role: &UserRole,
    real_name: Option<&str>,
) -> Result<UserProfile, sqlx::Error> {
    sqlx::query_as::<_, UserProfile>(&format!(
        r#"
        INSERT INTO users (username, email, password_hash, role, real_name)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING {}
        "#,
        PROFILE_COLS
    ))
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(role)
    .bind(real_name)
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateUser,
) -> Result<Option<UserProfile>, sqlx::Error> {
    sqlx::query_as::<_, UserProfile>(&format!(
        r#"
        UPDATE users
        SET
            email      = COALESCE($2, email),
            real_name  = COALESCE($3, real_name),
            avatar_url = COALESCE($4, avatar_url),
            is_active  = COALESCE($5, is_active),
            major_id   = COALESCE($6, major_id),
            role       = COALESCE($7, role),
            updated_at = NOW()
        WHERE id = $1
        RETURNING {}
        "#,
        PROFILE_COLS
    ))
    .bind(id)
    .bind(&payload.email)
    .bind(&payload.real_name)
    .bind(&payload.avatar_url)
    .bind(payload.is_active)
    .bind(payload.major_id)
    .bind(&payload.role)
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM users WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

/// 管理员重置用户密码：更新密码哈希并将 password_reset_required 置为 true
pub async fn reset_password(
    pool: &PgPool,
    id: Uuid,
    password_hash: &str,
) -> Result<Option<UserProfile>, sqlx::Error> {
    sqlx::query_as::<_, UserProfile>(&format!(
        r#"
        UPDATE users
        SET
            password_hash           = $2,
            password_reset_required = TRUE,
            updated_at              = NOW()
        WHERE id = $1
        RETURNING {}
        "#,
        PROFILE_COLS
    ))
    .bind(id)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}

/// 用户自行修改密码：更新密码哈希并将 password_reset_required 置为 false
pub async fn change_password(
    pool: &PgPool,
    id: Uuid,
    password_hash: &str,
) -> Result<Option<UserProfile>, sqlx::Error> {
    sqlx::query_as::<_, UserProfile>(&format!(
        r#"
        UPDATE users
        SET
            password_hash           = $2,
            password_reset_required = FALSE,
            updated_at              = NOW()
        WHERE id = $1
        RETURNING {}
        "#,
        PROFILE_COLS
    ))
    .bind(id)
    .bind(password_hash)
    .fetch_optional(pool)
    .await
}
