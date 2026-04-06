use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    major::{CreateMajor, Major, MajorWithStats, UpdateMajor},
    pagination::{PageQuery, PagedList},
};
use crate::repositories::major as major_repo;

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn bad_request(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

fn map_db_error(e: sqlx::Error) -> (StatusCode, String) {
    if let sqlx::Error::Database(db_err) = &e {
        if db_err.code().as_deref() == Some("23505") {
            return bad_request("专业代码已存在");
        }
    }
    internal_error(e)
}

/// GET /api/majors?page_size=20&cursor_created_at=...&cursor_id=...
/// 返回专业基本信息 + 课程数、报名学员数、视频总播放量
pub async fn list_majors(
    State(pool): State<PgPool>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<MajorWithStats>> {
    let result = major_repo::find_all(&pool, &query).await.map_err(internal_error)?;
    Ok(Json(result))
}

/// GET /api/majors/:id
/// 返回专业基本信息 + 统计数据
pub async fn get_major(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> AppResult<MajorWithStats> {
    let major = major_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("专业分类不存在"))?;
    Ok(Json(major))
}

/// POST /api/majors
pub async fn create_major(
    State(pool): State<PgPool>,
    Json(payload): Json<CreateMajor>,
) -> Result<(StatusCode, Json<Major>), (StatusCode, String)> {
    if payload.name.trim().is_empty() {
        return Err(bad_request("专业名称不能为空"));
    }

    let major = major_repo::create(&pool, &payload).await.map_err(map_db_error)?;
    Ok((StatusCode::CREATED, Json(major)))
}

/// PUT /api/majors/:id
pub async fn update_major(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateMajor>,
) -> AppResult<Major> {
    if let Some(name) = payload.name.as_deref() {
        if name.trim().is_empty() {
            return Err(bad_request("专业名称不能为空"));
        }
    }

    let major = major_repo::update(&pool, id, &payload)
        .await
        .map_err(map_db_error)?
        .ok_or_else(|| not_found("专业分类不存在"))?;

    Ok(Json(major))
}

/// DELETE /api/majors/:id
pub async fn delete_major(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let rows = major_repo::delete(&pool, id).await.map_err(internal_error)?;
    if rows == 0 {
        return Err(not_found("专业分类不存在"));
    }

    Ok(StatusCode::NO_CONTENT)
}
