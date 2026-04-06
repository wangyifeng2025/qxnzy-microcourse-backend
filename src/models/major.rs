use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Major {
    pub id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

/// 专业列表/详情响应（含统计数据）
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct MajorWithStats {
    pub id: Uuid,
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
    /// 已发布课程数
    pub course_count: i64,
    /// 报名过该专业任一课程的去重用户数
    pub enrolled_learner_count: i64,
    /// 该专业所有视频累计播放次数
    pub total_video_views: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateMajor {
    pub name: String,
    pub code: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateMajor {
    pub name: Option<String>,
    pub code: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}
