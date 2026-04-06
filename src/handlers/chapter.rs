use axum::{
    extract::{Extension, Path, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::{try_optional_auth_context_active, AuthContext};
use crate::models::course::{Chapter, Course, CreateChapterRequest, UpdateChapter};
use crate::repositories::chapter as chapter_repo;
use crate::repositories::course as course_repo;
use crate::models::enums::{CourseStatus, UserRole};

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

fn forbidden(msg: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, msg.to_string())
}

/// 与课程详情一致：仅已发布课程对匿名开放；草稿/归档需教师本人或管理员（带 Token）
fn course_visible_to(course: &Course, auth: Option<&AuthContext>) -> bool {
    if course.status == CourseStatus::Published {
        return true;
    }
    match auth {
        Some(a) if a.role == UserRole::Admin || a.user_id == course.teacher_id => true,
        _ => false,
    }
}

fn ensure_teacher_or_admin(
    auth: &AuthContext,
    course: &Course,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin || auth.user_id == course.teacher_id {
        return Ok(());
    }
    Err(forbidden("仅课程教师或管理员可操作该章节"))
}

/// GET /api/courses/:course_id/chapters
pub async fn list_chapters(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path(course_id): Path<Uuid>,
) -> AppResult<Vec<Chapter>> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;
    let auth = try_optional_auth_context_active(&pool, &headers)
        .await
        .map_err(internal_error)?;
    if !course_visible_to(&course, auth.as_ref()) {
        return Err(not_found("课程不存在"));
    }
    let chapters = chapter_repo::find_by_course_id(&pool, course_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(chapters))
}

/// GET /api/courses/:course_id/chapters/:chapter_id
pub async fn get_chapter(
    State(pool): State<PgPool>,
    headers: HeaderMap,
    Path((course_id, chapter_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Chapter> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;
    let auth = try_optional_auth_context_active(&pool, &headers)
        .await
        .map_err(internal_error)?;
    if !course_visible_to(&course, auth.as_ref()) {
        return Err(not_found("课程不存在"));
    }
    let chapter = chapter_repo::find_by_id(&pool, chapter_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("章节不存在"))?;
    if chapter.course_id != course_id {
        return Err(not_found("章节不存在"));
    }
    Ok(Json(chapter))
}

/// POST /api/courses/:course_id/chapters（仅教师/管理员，且须为课程创建者）
pub async fn create_chapter(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Json(payload): Json<CreateChapterRequest>,
) -> Result<(StatusCode, Json<Chapter>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("章节标题不能为空"));
    }

    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    let chapter = chapter_repo::create(&pool, course_id, &payload)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(chapter)))
}

/// PUT /api/courses/:course_id/chapters/:chapter_id
pub async fn update_chapter(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, chapter_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateChapter>,
) -> AppResult<Chapter> {
    if let Some(title) = payload.title.as_deref() {
        if title.trim().is_empty() {
            return Err(bad_request("章节标题不能为空"));
        }
    }

    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    let chapter = chapter_repo::find_by_id(&pool, chapter_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("章节不存在"))?;
    if chapter.course_id != course_id {
        return Err(not_found("章节不存在"));
    }

    let updated = chapter_repo::update(&pool, chapter_id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("章节不存在"))?;

    Ok(Json(updated))
}

/// DELETE /api/courses/:course_id/chapters/:chapter_id
pub async fn delete_chapter(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, chapter_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    let chapter = chapter_repo::find_by_id(&pool, chapter_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("章节不存在"))?;
    if chapter.course_id != course_id {
        return Err(not_found("章节不存在"));
    }

    chapter_repo::delete(&pool, chapter_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}
