use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::enums::CourseStatus;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Course {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
    pub teacher_id: Uuid,
    /// 来自 users.real_name 的 JOIN 字段，展示给前端用，不存储在 courses 表中
    pub teacher_name: Option<String>,
    pub status: CourseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vote_count: i64,
}

/// 课程 API 对外 JSON（`cover_image_url` 为可浏览器加载的地址：MinIO key 会转为预签名 GET）
#[derive(Debug, Clone, Serialize)]
pub struct CourseResponse {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
    pub teacher_id: Uuid,
    pub teacher_name: Option<String>,
    pub status: CourseStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub vote_count: i64,
    pub has_voted: bool,
}

#[derive(Debug, Deserialize)]
//这个是表单提交的数据
pub struct CreateCourse {
    pub title: String,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateCourse {
    pub title: Option<String>,
    pub description: Option<String>,
    pub cover_image_url: Option<String>,
    pub major_id: Option<Uuid>,
    pub status: Option<CourseStatus>,
}

/// POST /api/courses/:id/cover/upload-url
#[derive(Debug, Deserialize)]
pub struct CourseCoverUploadUrlRequest {
    pub filename: String,
}

/// POST /api/courses/:id/cover/upload-url 响应（与视频直传形态一致）
#[derive(Debug, Serialize)]
pub struct CourseCoverUploadUrlResponse {
    pub upload_url: String,
    pub object_key: String,
    pub expires_in: u64,
}

/// POST /api/courses/:id/cover/confirm
#[derive(Debug, Deserialize)]
pub struct CourseCoverConfirmRequest {
    pub object_key: String,
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Chapter {
    pub id: Uuid,
    pub course_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 创建章节的请求体（course_id 从路径获取）
#[derive(Debug, Deserialize)]
pub struct CreateChapterRequest {
    pub title: String,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateChapter {
    pub title: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

// ---------------------------------------------------------------------------

/// 学生选课记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct CourseEnrollment {
    pub id: Uuid,
    pub user_id: Uuid,
    pub course_id: Uuid,
    pub enrolled_at: DateTime<Utc>,
}

// ---------------------------------------------------------------------------

/// POST /api/courses/:id/vote 响应体
#[derive(Debug, Serialize)]
pub struct VoteStatusResponse {
    pub voted: bool,
    pub vote_count: i64,
}
