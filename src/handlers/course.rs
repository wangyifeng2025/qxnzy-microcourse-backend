use std::sync::Arc;

use axum::{
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, StatusCode},
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::{try_optional_auth_context_active, AuthContext};
use crate::models::course::{
    Course, CourseCoverConfirmRequest, CourseCoverUploadUrlRequest, CourseCoverUploadUrlResponse,
    CourseResponse, CreateCourse, UpdateCourse, VoteStatusResponse,
};
use crate::models::enums::{CourseStatus, UserRole};
use crate::models::pagination::{PageQuery, PagedList};
use crate::repositories::course as course_repo;
use crate::repositories::enrollment as enrollment_repo;
use crate::storage::AppStorage;
use crate::utils::filename::sanitize_filename;

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

/// 是否为后端管理的课程封面 object key（用于安全删除 MinIO 对象）
fn is_managed_course_cover_key(key: &str) -> bool {
    key.starts_with("course-covers/")
}

/// 校验图片扩展名（上传封面）
fn is_allowed_image_filename(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
}

/// 将库内封面字段转为前端可展示的 URL（MinIO key → 预签名 GET；已是 http(s) 则原样）。
/// 预签名失败时返回 `None` 并打日志，避免整页列表 500。
async fn resolve_cover_display_url(stored: Option<&str>, storage: &AppStorage) -> Option<String> {
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
            tracing::warn!(error = %e, key = %s, "课程封面预签名失败，该条目不返回封面 URL");
            None
        }
    }
}

async fn course_to_response(course: Course, storage: &AppStorage, has_voted: bool) -> CourseResponse {
    let cover_image_url = resolve_cover_display_url(course.cover_image_url.as_deref(), storage).await;
    CourseResponse {
        id: course.id,
        title: course.title,
        description: course.description,
        cover_image_url,
        major_id: course.major_id,
        teacher_id: course.teacher_id,
        teacher_name: course.teacher_name,
        status: course.status,
        created_at: course.created_at,
        updated_at: course.updated_at,
        vote_count: course.vote_count,
        has_voted,
    }
}

/// 未登录访客仅可看已发布课程；教师/管理员可看本人草稿等（与 list 规则一致）
fn course_visible_to(course: &Course, auth: Option<&AuthContext>) -> bool {
    if course.status == CourseStatus::Published {
        return true;
    }
    match auth {
        Some(a) if a.role == UserRole::Admin || a.user_id == course.teacher_id => true,
        _ => false,
    }
}

/// 校验当前用户是课程的教师本人或管理员
fn ensure_teacher_or_admin(
    auth: &AuthContext,
    course: &Course,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin || auth.user_id == course.teacher_id {
        return Ok(());
    }
    Err(forbidden("仅课程教师或管理员可操作该课程"))
}

/// GET /api/courses?page_size=20&cursor_created_at=...&cursor_id=...
/// 仅返回 **已发布** 课程（门户/首页）。管理端列表见 `GET /api/courses/manage`。
/// 接受可选 Authorization header，已登录 Student 时返回 has_voted
pub async fn list_courses(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    headers: HeaderMap,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<CourseResponse>> {
    let result = course_repo::find_all_published(&pool, &query)
        .await
        .map_err(internal_error)?;

    let auth = try_optional_auth_context_active(&pool, &headers)
        .await
        .map_err(internal_error)?;

    let voted_set = match &auth {
        Some(a) if a.role == UserRole::Student => {
            let ids: Vec<Uuid> = result.items.iter().map(|c| c.id).collect();
            course_repo::batch_get_voted_courses(&pool, a.user_id, &ids)
                .await
                .map_err(internal_error)?
        }
        _ => std::collections::HashSet::new(),
    };

    let mut items = Vec::with_capacity(result.items.len());
    for c in result.items {
        let has_voted = voted_set.contains(&c.id);
        items.push(course_to_response(c, &storage, has_voted).await);
    }
    Ok(Json(PagedList {
        page_size: result.page_size,
        has_more: result.has_more,
        next_cursor_created_at: result.next_cursor_created_at,
        next_cursor_id: result.next_cursor_id,
        items,
    }))
}

/// GET /api/courses/manage?...（需登录，教师/管理员）
/// 教师：本人所有状态课程；管理员：全站所有课程。
pub async fn list_courses_manage(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<CourseResponse>> {
    let teacher_filter = if auth.role == UserRole::Admin {
        None
    } else {
        Some(auth.user_id)
    };
    let result = course_repo::find_all_managed(&pool, &query, teacher_filter)
        .await
        .map_err(internal_error)?;
    let mut items = Vec::with_capacity(result.items.len());
    for c in result.items {
        items.push(course_to_response(c, &storage, false).await);
    }
    Ok(Json(PagedList {
        page_size: result.page_size,
        has_more: result.has_more,
        next_cursor_created_at: result.next_cursor_created_at,
        next_cursor_id: result.next_cursor_id,
        items,
    }))
}

/// GET /api/courses/:id
/// 未发布课程仅教师本人或管理员可见（请求可带 `Authorization: Bearer`）。
pub async fn get_course(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    headers: HeaderMap,
    Path(id): Path<Uuid>,
) -> AppResult<CourseResponse> {
    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;
    let auth = try_optional_auth_context_active(&pool, &headers)
        .await
        .map_err(internal_error)?;
    if !course_visible_to(&course, auth.as_ref()) {
        return Err(not_found("课程不存在"));
    }
    let has_voted = match &auth {
        Some(a) if a.role == UserRole::Student => {
            course_repo::batch_get_voted_courses(&pool, a.user_id, &[course.id])
                .await
                .map_err(internal_error)?
                .contains(&course.id)
        }
        _ => false,
    };
    let out = course_to_response(course, &storage, has_voted).await;
    Ok(Json(out))
}

/// POST /api/courses（仅 Teacher / Admin）
pub async fn create_course(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Json(payload): Json<CreateCourse>,
) -> Result<(StatusCode, Json<CourseResponse>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("课程标题不能为空"));
    }

    let course = course_repo::create(&pool, auth.user_id, &payload)
        .await
        .map_err(internal_error)?;

    let out = course_to_response(course, &storage, false).await;

    Ok((StatusCode::CREATED, Json(out)))
}

/// PUT /api/courses/:id（仅课程教师本人 / Admin）
pub async fn update_course(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateCourse>,
) -> AppResult<CourseResponse> {
    if let Some(title) = payload.title.as_deref() {
        if title.trim().is_empty() {
            return Err(bad_request("课程标题不能为空"));
        }
    }

    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    // 若通过 PUT 直接更换 cover_image_url，删除旧的 MinIO 对象（仅管理的路径）
    if let Some(ref new_key) = payload.cover_image_url {
        if course.cover_image_url.as_ref() != Some(new_key) {
            if let Some(ref old_key) = course.cover_image_url {
                if is_managed_course_cover_key(old_key) && old_key != new_key {
                    if let Err(e) = storage.delete_object(old_key).await {
                        tracing::warn!(error = %e, key = %old_key, "删除旧课程封面失败");
                    }
                }
            }
        }
    }

    let updated = course_repo::update(&pool, id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let out = course_to_response(updated, &storage, false).await;
    Ok(Json(out))
}

/// POST /api/courses/:id/cover/upload-url（预签名 PUT，写入待确认的 object key）
pub async fn request_course_cover_upload_url(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CourseCoverUploadUrlRequest>,
) -> AppResult<CourseCoverUploadUrlResponse> {
    if payload.filename.trim().is_empty() {
        return Err(bad_request("filename 不能为空"));
    }
    let safe_name = sanitize_filename(&payload.filename);
    if !is_allowed_image_filename(&safe_name) {
        return Err(bad_request("仅支持 jpg、jpeg、png、webp、gif 作为封面"));
    }

    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    let object_key = format!("course-covers/{id}/{safe_name}");

    // 若已存在旧封面且本次为新 key，删除 MinIO 上旧对象（与视频申请 URL 时替换逻辑一致）
    if let Some(ref old_key) = course.cover_image_url {
        if old_key != &object_key && is_managed_course_cover_key(old_key) {
            if let Err(e) = storage.delete_object(old_key).await {
                tracing::warn!(error = %e, key = %old_key, "删除待替换的课程封面失败");
            }
        }
    }

    course_repo::set_cover_image_url(&pool, id, &object_key)
        .await
        .map_err(internal_error)?;

    const EXPIRES_SECS: u64 = 3600;
    let upload_url = storage
        .presigned_put_url(&object_key, EXPIRES_SECS)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    Ok(Json(CourseCoverUploadUrlResponse {
        upload_url,
        object_key,
        expires_in: EXPIRES_SECS,
    }))
}

/// POST /api/courses/:id/cover/confirm
pub async fn confirm_course_cover(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CourseCoverConfirmRequest>,
) -> AppResult<serde_json::Value> {
    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    let stored_key = course
        .cover_image_url
        .as_deref()
        .ok_or_else(|| bad_request("请先申请封面上传 URL"))?;

    if stored_key != payload.object_key {
        return Err(bad_request("object_key 与记录不一致"));
    }

    if !storage.object_exists(&payload.object_key).await {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "MinIO 中尚未找到该封面文件，请确认上传已完成".to_string(),
        ));
    }

    course_repo::touch_course_updated(&pool, id)
        .await
        .map_err(internal_error)?;

    let display_url = resolve_cover_display_url(Some(stored_key), &storage)
        .await
        .unwrap_or_else(|| stored_key.to_string());

    Ok(Json(serde_json::json!({
        "message": "课程封面上传已确认",
        "cover_image_url": display_url,
    })))
}

/// DELETE /api/courses/:id/cover
pub async fn delete_course_cover(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    if let Some(ref key) = course.cover_image_url {
        if is_managed_course_cover_key(key) {
            if let Err(e) = storage.delete_object(key).await {
                tracing::warn!(error = %e, key = %key, "删除课程封面对象失败");
            }
        }
    }

    course_repo::clear_cover_image_url(&pool, id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// DELETE /api/courses/:id（仅课程教师本人 / Admin）
pub async fn delete_course(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_teacher_or_admin(&auth, &course)?;

    if let Some(ref key) = course.cover_image_url {
        if is_managed_course_cover_key(key) {
            if let Err(e) = storage.delete_object(key).await {
                tracing::warn!(error = %e, key = %key, "删除课程封面对象失败");
            }
        }
    }

    course_repo::delete(&pool, id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/courses/:id/vote（仅 UserRole::Student 且已选课）
/// 切换投票状态，返回操作后的 voted 和 vote_count
pub async fn toggle_course_vote(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
) -> AppResult<VoteStatusResponse> {
    if auth.role != UserRole::Student {
        return Err(forbidden("仅学生可以对课程投票"));
    }

    course_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let enrolled = enrollment_repo::find_by_user_and_course(&pool, auth.user_id, id)
        .await
        .map_err(internal_error)?;
    if enrolled.is_none() {
        return Err(forbidden("仅已选课的学生可以投票"));
    }

    let (voted, vote_count) = course_repo::toggle_vote(&pool, auth.user_id, id)
        .await
        .map_err(internal_error)?;

    Ok(Json(VoteStatusResponse { voted, vote_count }))
}
