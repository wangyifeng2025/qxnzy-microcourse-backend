use std::collections::HashSet;
use std::sync::Arc;

use axum::{
    Extension,
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::course::{CourseEnrollment, CourseResponse};
use crate::models::enums::{CourseStatus, UserRole};
use crate::models::pagination::{PageQuery, PagedList};
use crate::repositories::course as course_repo;
use crate::repositories::enrollment as enrollment_repo;
use crate::storage::AppStorage;

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

/// 将课程封面 MinIO key 转为预签名 GET URL（与 course handler 逻辑一致）
async fn resolve_cover_url(stored: Option<&str>, storage: &AppStorage) -> Option<String> {
    let s = stored?.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Some(s.to_string());
    }
    const TTL_SECS: u64 = 3600 * 24 * 7;
    match storage.presigned_get_url(s, TTL_SECS).await {
        Ok(url) => Some(url),
        Err(e) => {
            tracing::warn!(error = %e, key = %s, "已选课程封面预签名失败");
            None
        }
    }
}

/// GET /api/courses/enrolled — 当前登录用户的已选课程列表（游标分页）
pub async fn list_enrolled_courses(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<CourseResponse>> {
    let result = enrollment_repo::find_enrolled_courses_by_user(&pool, auth.user_id, &query)
        .await
        .map_err(internal_error)?;

    let voted_set: HashSet<Uuid> = if auth.role == UserRole::Student {
        let ids: Vec<Uuid> = result.items.iter().map(|c| c.id).collect();
        course_repo::batch_get_voted_courses(&pool, auth.user_id, &ids)
            .await
            .map_err(internal_error)?
    } else {
        HashSet::new()
    };

    let mut items = Vec::with_capacity(result.items.len());
    for c in result.items {
        let cover_image_url = resolve_cover_url(c.cover_image_url.as_deref(), &storage).await;
        let has_voted = voted_set.contains(&c.id);
        items.push(CourseResponse {
            id: c.id,
            title: c.title,
            description: c.description,
            cover_image_url,
            major_id: c.major_id,
            teacher_id: c.teacher_id,
            teacher_name: c.teacher_name,
            status: c.status,
            created_at: c.created_at,
            updated_at: c.updated_at,
            vote_count: c.vote_count,
            has_voted,
        });
    }

    Ok(Json(PagedList {
        page_size: result.page_size,
        has_more: result.has_more,
        next_cursor_created_at: result.next_cursor_created_at,
        next_cursor_id: result.next_cursor_id,
        items,
    }))
}

/// GET /api/courses/:course_id/enrollment
/// 查询当前登录用户是否已选该课程
pub async fn get_enrollment_status(
    State(pool): State<PgPool>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> AppResult<serde_json::Value> {
    course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let enrollment = enrollment_repo::find_by_user_and_course(&pool, auth.user_id, course_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "enrolled": enrollment.is_some(),
        "enrollment": enrollment,
    })))
}

/// GET /api/courses/:course_id/enroll/count
/// 查询某门课程的选课总人数（任意已登录用户可访问）
pub async fn get_course_enrollment_count(
    State(pool): State<PgPool>,
    _auth: axum::Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> AppResult<serde_json::Value> {
    course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let count = enrollment_repo::count_by_course(&pool, course_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "course_id": course_id,
        "enrollment_count": count,
    })))
}

/// POST /api/courses/:course_id/enroll
/// 当前登录用户选课（仅限已发布课程）
pub async fn enroll_course(
    State(pool): State<PgPool>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> Result<(StatusCode, Json<CourseEnrollment>), (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if course.status != CourseStatus::Published {
        return Err((StatusCode::UNPROCESSABLE_ENTITY, "该课程尚未发布，暂不可选课".to_string()));
    }

    let enrollment = enrollment_repo::enroll(&pool, auth.user_id, course_id)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(enrollment)))
}

/// DELETE /api/courses/:course_id/enroll
/// 当前登录用户取消选课
pub async fn unenroll_course(
    State(pool): State<PgPool>,
    axum::Extension(auth): axum::Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let deleted = enrollment_repo::unenroll_with_vote_cascade(&pool, auth.user_id, course_id)
        .await
        .map_err(internal_error)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(not_found("未找到该选课记录"))
    }
}
