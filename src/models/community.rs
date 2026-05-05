use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::enums::UserRole;

// ---------------------------------------------------------------------------
// 话题专用分页查询参数
// ---------------------------------------------------------------------------

/// 话题列表专用游标分页参数（排序键为 is_pinned DESC, created_at DESC, id DESC）。
/// 需在游标中同时携带 `cursor_is_pinned`，以避免跨越置顶/非置顶边界时丢数据。
#[derive(Debug, Clone, Deserialize)]
pub struct TopicPageQuery {
    /// 每页条数，默认 20，最大 100
    #[serde(default = "default_page_size")]
    pub page_size: i64,
    pub cursor_created_at: Option<DateTime<Utc>>,
    pub cursor_id: Option<Uuid>,
    /// 上一页最后一条话题的 is_pinned 值（配合 cursor_created_at / cursor_id 使用）
    pub cursor_is_pinned: Option<bool>,
}

fn default_page_size() -> i64 {
    20
}

impl TopicPageQuery {
    pub fn page_size(&self) -> i64 {
        self.page_size.clamp(1, 100)
    }
}

/// GET /api/courses/:course_id/topics/highlight — 门户展示用，无需登录
#[derive(Debug, Clone, Deserialize)]
pub struct TopicHighlightQuery {
    #[serde(default = "default_highlight_limit")]
    pub limit: i64,
}

fn default_highlight_limit() -> i64 {
    5
}

impl TopicHighlightQuery {
    pub fn limit(&self) -> i64 {
        self.limit.clamp(1, 10)
    }
}

/// 课程讨论精选（公开摘要）
#[derive(Debug, Clone, Serialize)]
pub struct CourseTopicHighlightResponse {
    pub id: Uuid,
    pub title: String,
    pub content_preview: String,
    pub reply_count: i32,
    pub author_display: String,
}

// ---------------------------------------------------------------------------
// 话题（Topic）
// ---------------------------------------------------------------------------

/// 话题原始行（用于内部逻辑）
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CourseTopic {
    pub id: Uuid,
    pub course_id: Uuid,
    pub author_id: Uuid,
    pub title: String,
    pub content: String,
    pub is_pinned: bool,
    pub reply_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 话题 API 响应（JOIN users 得到作者信息）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CourseTopicResponse {
    pub id: Uuid,
    pub course_id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_real_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: UserRole,
    pub title: String,
    pub content: String,
    pub is_pinned: bool,
    pub reply_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// POST /api/courses/:course_id/topics 请求体
#[derive(Debug, Deserialize)]
pub struct CreateTopicRequest {
    pub title: String,
    pub content: String,
    /// 本帖中被 @ 的用户 ID 列表（需为同课程成员）
    #[serde(default)]
    pub mention_user_ids: Vec<Uuid>,
}

// ---------------------------------------------------------------------------
// 回复（Reply）
// ---------------------------------------------------------------------------

/// 回复 API 响应（JOIN users 得到作者信息）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CourseTopicReplyResponse {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_real_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: UserRole,
    pub content: String,
    /// 若非空，表示回复的是某条具体回复（楼中楼）
    pub reply_to_reply_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// POST /api/courses/:course_id/topics/:topic_id/replies 请求体
#[derive(Debug, Deserialize)]
pub struct CreateReplyRequest {
    pub content: String,
    /// 若非空，表示回复某条具体的回复
    pub reply_to_reply_id: Option<Uuid>,
    /// 本条回复中被 @ 的用户 ID 列表（需为同课程成员）
    #[serde(default)]
    pub mention_user_ids: Vec<Uuid>,
}

// ---------------------------------------------------------------------------
// 课程社区成员（用于 @ 候选列表）
// ---------------------------------------------------------------------------

/// 课程社区可 @ 成员（已选课学生 + 课程教师）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CourseMember {
    pub user_id: Uuid,
    pub username: String,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: UserRole,
}

// ---------------------------------------------------------------------------
// 独立社区话题（不依附课程）
// ---------------------------------------------------------------------------

/// 独立社区话题 API 响应
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CommunityTopicResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_real_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: UserRole,
    pub title: String,
    pub content: String,
    pub is_pinned: bool,
    pub reply_count: i32,
    pub member_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// POST /api/community/topics 请求体
#[derive(Debug, Deserialize)]
pub struct CreateCommunityTopicRequest {
    pub title: String,
    pub content: String,
    /// 本帖中被 @ 的用户 ID 列表（需为已注册用户）
    #[serde(default)]
    pub mention_user_ids: Vec<Uuid>,
}

/// 独立社区话题成员
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CommunityTopicMember {
    pub user_id: Uuid,
    pub username: String,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: UserRole,
    pub joined_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------
// 独立社区话题回复
// ---------------------------------------------------------------------------

/// 独立社区话题回复 API 响应
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct CommunityTopicReplyResponse {
    pub id: Uuid,
    pub topic_id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_real_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: UserRole,
    pub content: String,
    pub reply_to_reply_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// POST /api/community/topics/:id/replies 请求体
#[derive(Debug, Deserialize)]
pub struct CreateCommunityTopicReplyRequest {
    pub content: String,
    pub reply_to_reply_id: Option<Uuid>,
    /// 被 @ 的用户 ID 列表（需为该话题成员）
    #[serde(default)]
    pub mention_user_ids: Vec<Uuid>,
}
