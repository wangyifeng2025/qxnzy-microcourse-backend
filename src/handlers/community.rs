use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::community::{
    CourseMember, CommunityTopicMember, CommunityTopicReplyResponse, CommunityTopicResponse,
    CourseTopicHighlightResponse, CourseTopicReplyResponse, CourseTopicResponse,
    CreateCommunityTopicReplyRequest, CreateCommunityTopicRequest, CreateReplyRequest,
    CreateTopicRequest, TopicHighlightQuery, TopicPageQuery,
};
use crate::models::enums::{CourseStatus, UserRole};
use crate::models::pagination::{PageQuery, PagedList};
use crate::repositories::community::{self as community_repo, StandaloneTopicPagedList, TopicPagedList};
use crate::repositories::course as course_repo;

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

// ---------------------------------------------------------------------------
// 权限辅助
// ---------------------------------------------------------------------------

/// 校验用户是否为课程社区成员（教师本人 / 已选课学生 / 管理员）。
/// 调用方须事先确保 course 已存在。
async fn ensure_community_access(
    pool: &PgPool,
    auth: &AuthContext,
    course_teacher_id: Uuid,
    course_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin || auth.user_id == course_teacher_id {
        return Ok(());
    }
    if auth.role == UserRole::Student {
        let enrolled = community_repo::is_community_member(pool, auth.user_id, course_id)
            .await
            .map_err(internal_error)?;
        if enrolled {
            return Ok(());
        }
    }
    Err(forbidden("仅课程教师、已选课学生或管理员可访问课程社区"))
}

// ---------------------------------------------------------------------------
// 话题接口
// ---------------------------------------------------------------------------

/// GET /api/courses/:course_id/topics
/// 分页列出话题（置顶优先，然后按发布时间倒序）。
/// 游标参数：`cursor_is_pinned`、`cursor_created_at`、`cursor_id` 三者需同时提供或同时省略。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn list_topics(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Query(query): Query<TopicPageQuery>,
) -> AppResult<TopicPagedList> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    let result = community_repo::list_topics(&pool, course_id, &query)
        .await
        .map_err(internal_error)?;

    Ok(Json(result))
}

/// GET /api/courses/:course_id/topics/highlight?limit=5
/// 无需登录；仅已发布课程返回话题摘要，供门户课程详情侧栏「讨论精选」。
pub async fn list_topic_highlights(
    State(pool): State<PgPool>,
    Path(course_id): Path<Uuid>,
    Query(query): Query<TopicHighlightQuery>,
) -> AppResult<Vec<CourseTopicHighlightResponse>> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if course.status != CourseStatus::Published {
        return Err(not_found("课程不存在"));
    }

    let items = community_repo::list_topic_highlights_public(&pool, course_id, query.limit())
        .await
        .map_err(internal_error)?;

    Ok(Json(items))
}

/// POST /api/courses/:course_id/topics
/// 发布话题。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn create_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Json(payload): Json<CreateTopicRequest>,
) -> Result<(StatusCode, Json<CourseTopicResponse>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("话题标题不能为空"));
    }
    if payload.content.trim().is_empty() {
        return Err(bad_request("话题内容不能为空"));
    }

    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    // 校验 @提及的用户都是该课程的合法成员
    if !payload.mention_user_ids.is_empty() {
        let invalid = community_repo::validate_mention_users(
            &pool,
            course_id,
            &payload.mention_user_ids,
        )
        .await
        .map_err(internal_error)?;
        if !invalid.is_empty() {
            return Err(bad_request("@提及的用户中存在非本课程成员"));
        }
    }

    let topic = community_repo::create_topic(
        &pool,
        course_id,
        auth.user_id,
        payload.title.trim(),
        payload.content.trim(),
        &payload.mention_user_ids,
    )
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(topic)))
}

/// GET /api/courses/:course_id/topics/:topic_id
/// 获取话题详情。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn get_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, topic_id)): Path<(Uuid, Uuid)>,
) -> AppResult<CourseTopicResponse> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    let topic = community_repo::find_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    if topic.course_id != course_id {
        return Err(not_found("话题不存在"));
    }

    Ok(Json(topic))
}

/// DELETE /api/courses/:course_id/topics/:topic_id
/// 删除话题（同时级联删除所有回复和提及记录）。
/// 权限：话题作者 / 课程教师 / 管理员。
pub async fn delete_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, topic_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let topic = community_repo::find_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    if topic.course_id != course_id {
        return Err(not_found("话题不存在"));
    }

    // 允许删除：话题作者本人、课程教师、管理员
    let can_delete = auth.role == UserRole::Admin
        || auth.user_id == course.teacher_id
        || auth.user_id == topic.author_id;
    if !can_delete {
        return Err(forbidden("仅话题作者、课程教师或管理员可删除该话题"));
    }

    community_repo::delete_topic(&pool, topic_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 回复接口
// ---------------------------------------------------------------------------

/// GET /api/courses/:course_id/topics/:topic_id/replies
/// 分页获取话题回复（时间正序）。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn list_replies(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, topic_id)): Path<(Uuid, Uuid)>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<CourseTopicReplyResponse>> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    // 确认话题存在且属于该课程
    let topic = community_repo::find_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;
    if topic.course_id != course_id {
        return Err(not_found("话题不存在"));
    }

    let result = community_repo::list_replies(&pool, topic_id, &query)
        .await
        .map_err(internal_error)?;

    Ok(Json(result))
}

/// POST /api/courses/:course_id/topics/:topic_id/replies
/// 发布回复。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn create_reply(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, topic_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<CreateReplyRequest>,
) -> Result<(StatusCode, Json<CourseTopicReplyResponse>), (StatusCode, String)> {
    if payload.content.trim().is_empty() {
        return Err(bad_request("回复内容不能为空"));
    }

    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    let topic = community_repo::find_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;
    if topic.course_id != course_id {
        return Err(not_found("话题不存在"));
    }

    // 若指定了楼中楼的目标回复，校验其确实属于该话题
    if let Some(ref_reply_id) = payload.reply_to_reply_id {
        let ref_reply = community_repo::find_reply_by_id(&pool, ref_reply_id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| not_found("被回复的评论不存在"))?;
        if ref_reply.topic_id != topic_id {
            return Err(bad_request("被回复的评论不属于该话题"));
        }
    }

    // 校验 @提及的用户都是该课程的合法成员
    if !payload.mention_user_ids.is_empty() {
        let invalid = community_repo::validate_mention_users(
            &pool,
            course_id,
            &payload.mention_user_ids,
        )
        .await
        .map_err(internal_error)?;
        if !invalid.is_empty() {
            return Err(bad_request("@提及的用户中存在非本课程成员"));
        }
    }

    let reply = community_repo::create_reply(
        &pool,
        topic_id,
        auth.user_id,
        payload.content.trim(),
        payload.reply_to_reply_id,
        &payload.mention_user_ids,
    )
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(reply)))
}

/// DELETE /api/courses/:course_id/topics/:topic_id/replies/:reply_id
/// 删除回复。
/// 权限：回复作者本人 / 课程教师 / 管理员。
pub async fn delete_reply(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, topic_id, reply_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    let reply = community_repo::find_reply_by_id(&pool, reply_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("回复不存在"))?;

    if reply.topic_id != topic_id {
        return Err(not_found("回复不存在"));
    }

    // 确认话题属于该课程
    let topic = community_repo::find_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;
    if topic.course_id != course_id {
        return Err(not_found("话题不存在"));
    }

    let can_delete = auth.role == UserRole::Admin
        || auth.user_id == course.teacher_id
        || auth.user_id == reply.author_id;
    if !can_delete {
        return Err(forbidden("仅回复作者、课程教师或管理员可删除该回复"));
    }

    community_repo::delete_reply(&pool, reply_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 课程社区成员（@提及候选）
// ---------------------------------------------------------------------------

/// GET /api/courses/:course_id/community/members
/// 获取该课程可 @ 的成员列表（已选课学生 + 课程教师，按用户名排序）。
/// 权限：课程教师 / 已选课学生 / 管理员。
pub async fn list_community_members(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> AppResult<Vec<CourseMember>> {
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    ensure_community_access(&pool, &auth, course.teacher_id, course_id).await?;

    let members = community_repo::list_course_members(&pool, course_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(members))
}

// ===========================================================================
// 独立社区话题接口（/api/community/topics）
// ===========================================================================

// ---------------------------------------------------------------------------
// 权限辅助
// ---------------------------------------------------------------------------

/// 确认当前用户是话题成员（或管理员）。
async fn ensure_standalone_topic_member(
    pool: &PgPool,
    auth: &AuthContext,
    topic_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin {
        return Ok(());
    }
    let is_member = community_repo::is_standalone_topic_member(pool, auth.user_id, topic_id)
        .await
        .map_err(internal_error)?;
    if is_member {
        return Ok(());
    }
    Err(forbidden("仅话题成员或管理员可访问此内容"))
}

// ---------------------------------------------------------------------------
// 话题接口
// ---------------------------------------------------------------------------

/// GET /api/community/topics
/// 分页列出独立社区话题（置顶优先，然后按发布时间倒序）。
/// 权限：任意已登录用户。
pub async fn list_standalone_topics(
    State(pool): State<PgPool>,
    Extension(_auth): Extension<AuthContext>,
    Query(query): Query<TopicPageQuery>,
) -> AppResult<StandaloneTopicPagedList> {
    let result = community_repo::list_standalone_topics(&pool, &query)
        .await
        .map_err(internal_error)?;
    Ok(Json(result))
}

/// POST /api/community/topics
/// 开设独立社区话题。
/// 权限：教师 / 管理员。
pub async fn create_standalone_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Json(payload): Json<CreateCommunityTopicRequest>,
) -> Result<(StatusCode, Json<CommunityTopicResponse>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("话题标题不能为空"));
    }
    if payload.content.trim().is_empty() {
        return Err(bad_request("话题内容不能为空"));
    }

    let topic = community_repo::create_standalone_topic(
        &pool,
        auth.user_id,
        payload.title.trim(),
        payload.content.trim(),
        &payload.mention_user_ids,
    )
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(topic)))
}

/// GET /api/community/topics/:topic_id
/// 获取独立社区话题详情。
/// 权限：任意已登录用户。
pub async fn get_standalone_topic(
    State(pool): State<PgPool>,
    Extension(_auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
) -> AppResult<CommunityTopicResponse> {
    let topic = community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;
    Ok(Json(topic))
}

/// DELETE /api/community/topics/:topic_id
/// 删除独立社区话题（级联删除所有回复和提及记录）。
/// 权限：话题作者 / 管理员。
pub async fn delete_standalone_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let topic = community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    let can_delete = auth.role == UserRole::Admin || auth.user_id == topic.author_id;
    if !can_delete {
        return Err(forbidden("仅话题作者或管理员可删除该话题"));
    }

    community_repo::delete_standalone_topic(&pool, topic_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 成员接口
// ---------------------------------------------------------------------------

/// POST /api/community/topics/:topic_id/join
/// 加入独立社区话题。
/// 权限：任意已登录用户（已加入则返回 200）。
pub async fn join_standalone_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    // 先确认话题存在
    community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    community_repo::join_standalone_topic(&pool, auth.user_id, topic_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::OK)
}

/// DELETE /api/community/topics/:topic_id/join
/// 退出独立社区话题。
/// 权限：话题成员（话题作者不可退出）。
pub async fn leave_standalone_topic(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let topic = community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    if auth.user_id == topic.author_id {
        return Err(bad_request("话题创建者不可退出自己开设的话题"));
    }

    let left = community_repo::leave_standalone_topic(&pool, auth.user_id, topic_id)
        .await
        .map_err(internal_error)?;

    if !left {
        return Err(bad_request("您尚未加入该话题"));
    }

    Ok(StatusCode::NO_CONTENT)
}

/// GET /api/community/topics/:topic_id/members
/// 获取话题成员列表。
/// 权限：话题成员 / 管理员。
pub async fn list_standalone_topic_members(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
) -> AppResult<Vec<CommunityTopicMember>> {
    community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    ensure_standalone_topic_member(&pool, &auth, topic_id).await?;

    let members = community_repo::list_standalone_topic_members(&pool, topic_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(members))
}

// ---------------------------------------------------------------------------
// 回复接口
// ---------------------------------------------------------------------------

/// GET /api/community/topics/:topic_id/replies
/// 分页获取话题回复（时间正序）。
/// 权限：话题成员 / 管理员。
pub async fn list_standalone_topic_replies(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
    Query(query): Query<PageQuery>,
) -> AppResult<PagedList<CommunityTopicReplyResponse>> {
    community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    ensure_standalone_topic_member(&pool, &auth, topic_id).await?;

    let result = community_repo::list_standalone_topic_replies(&pool, topic_id, &query)
        .await
        .map_err(internal_error)?;

    Ok(Json(result))
}

/// POST /api/community/topics/:topic_id/replies
/// 发布回复。
/// 权限：话题成员 / 管理员。
pub async fn create_standalone_topic_reply(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(topic_id): Path<Uuid>,
    Json(payload): Json<CreateCommunityTopicReplyRequest>,
) -> Result<(StatusCode, Json<CommunityTopicReplyResponse>), (StatusCode, String)> {
    if payload.content.trim().is_empty() {
        return Err(bad_request("回复内容不能为空"));
    }

    community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    ensure_standalone_topic_member(&pool, &auth, topic_id).await?;

    // 若指定了楼中楼的目标回复，校验其确实属于该话题
    if let Some(ref_reply_id) = payload.reply_to_reply_id {
        let ref_reply = community_repo::find_standalone_topic_reply_by_id(&pool, ref_reply_id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| not_found("被回复的评论不存在"))?;
        if ref_reply.topic_id != topic_id {
            return Err(bad_request("被回复的评论不属于该话题"));
        }
    }

    // 校验 @提及的用户都是该话题成员
    if !payload.mention_user_ids.is_empty() {
        let invalid = community_repo::validate_standalone_topic_mention_users(
            &pool,
            topic_id,
            &payload.mention_user_ids,
        )
        .await
        .map_err(internal_error)?;
        if !invalid.is_empty() {
            return Err(bad_request("@提及的用户中存在非话题成员"));
        }
    }

    let reply = community_repo::create_standalone_topic_reply(
        &pool,
        topic_id,
        auth.user_id,
        payload.content.trim(),
        payload.reply_to_reply_id,
        &payload.mention_user_ids,
    )
    .await
    .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(reply)))
}

/// DELETE /api/community/topics/:topic_id/replies/:reply_id
/// 删除回复。
/// 权限：回复作者 / 话题作者 / 管理员。
pub async fn delete_standalone_topic_reply(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((topic_id, reply_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    let topic = community_repo::find_standalone_topic_by_id(&pool, topic_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("话题不存在"))?;

    let reply = community_repo::find_standalone_topic_reply_by_id(&pool, reply_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("回复不存在"))?;

    if reply.topic_id != topic_id {
        return Err(not_found("回复不存在"));
    }

    let can_delete = auth.role == UserRole::Admin
        || auth.user_id == reply.author_id
        || auth.user_id == topic.author_id;
    if !can_delete {
        return Err(forbidden("仅回复作者、话题作者或管理员可删除该回复"));
    }

    community_repo::delete_standalone_topic_reply(&pool, reply_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}
