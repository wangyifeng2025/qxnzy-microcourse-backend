use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::enums::{CourseStatus, UserRole};

// ---------------------------------------------------------------------------
// 查询参数
// ---------------------------------------------------------------------------

/// 发现类接口通用 limit 参数（默认 10，最大 50）
#[derive(Debug, Clone, Deserialize)]
pub struct DiscoverLimitQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    10
}

impl DiscoverLimitQuery {
    pub fn limit(&self) -> i64 {
        self.limit.clamp(1, 50)
    }
}

// ---------------------------------------------------------------------------
// 热门课程
//
// 评分规则（见 repositories/discover.rs 注释）：
//   score = enrollment_count * 2 + vote_count
// 仅统计已发布课程。
// ---------------------------------------------------------------------------

/// 供 sqlx 从数据库行反序列化的内部类型
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PopularCourseRow {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    /// 数据库存储的 MinIO key 或外部 URL，由 handler 层转换为预签名 URL
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
    pub teacher_id: Uuid,
    pub teacher_name: Option<String>,
    pub status: CourseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vote_count: i64,
    pub enrollment_count: i64,
    pub major_name: Option<String>,
}

/// API 响应体（cover_image_url 已替换为可访问 URL）
#[derive(Debug, Clone, Serialize)]
pub struct PopularCourseResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
    pub major_name: Option<String>,
    pub teacher_id: Uuid,
    pub teacher_name: Option<String>,
    pub status: CourseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vote_count: i64,
    pub enrollment_count: i64,
}

// ---------------------------------------------------------------------------
// 活跃教师
//
// 判定标准（见 repositories/discover.rs 注释）：
//   1. 至少有 1 门已发布课程
//   2. 一级排序：旗下已发布课程的总选课人数 total_student_count（越多越活跃）
//   3. 二级排序：近 30 天在课程社区或独立社区发布的话题数 + 回复数 recent_activity_count
//   4. 三级排序：已发布课程数 published_course_count
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ActiveTeacherResponse {
    pub user_id: Uuid,
    pub username: String,
    pub real_name: Option<String>,
    pub avatar_url: Option<String>,
    /// 已发布课程数
    pub published_course_count: i64,
    /// 旗下已发布课程的累计选课人数（去重）
    pub total_student_count: i64,
    /// 近 30 天在各社区的话题数 + 回复数
    pub recent_activity_count: i64,
}

// ---------------------------------------------------------------------------
// 最新话题
//
// 同时汇总「课程社区话题」（course_topics）和
// 「独立社区话题」（community_topics），按 created_at DESC 排序。
// source = "course"   → source_id 为课程 ID，source_title 为课程标题
// source = "community"→ source_id / source_title 均为 NULL
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LatestTopicResponse {
    pub id: Uuid,
    pub author_id: Uuid,
    pub author_username: String,
    pub author_real_name: Option<String>,
    pub author_avatar_url: Option<String>,
    pub author_role: UserRole,
    pub title: String,
    /// 内容前 200 个字符（预览）
    pub content_preview: String,
    pub reply_count: i32,
    pub created_at: DateTime<Utc>,
    /// "course" 或 "community"
    pub source: String,
    /// 仅 source = "course" 时有值：课程 ID
    pub source_id: Option<Uuid>,
    /// 仅 source = "course" 时有值：课程标题
    pub source_title: Option<String>,
}
