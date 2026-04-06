use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 学习进度：每个用户对每个视频的观看进度
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LearningProgress {
    pub id: Uuid,
    pub user_id: Uuid,
    pub video_id: Uuid,
    /// 上次播放位置（秒）
    pub last_position: i32,
    /// 累计观看时长（秒）
    pub watched_duration: i32,
    /// 完成百分比，0-100，精度 NUMERIC(5,2)
    pub progress_pct: Decimal,
    pub is_completed: bool,
    pub completed_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// 上报/更新学习进度时的输入
#[derive(Debug, Deserialize)]
pub struct UpsertLearningProgress {
    pub video_id: Uuid,
    /// 当前播放位置（秒）
    pub last_position: i32,
    /// 本次新增观看时长（秒）
    pub watched_duration: i32,
}
