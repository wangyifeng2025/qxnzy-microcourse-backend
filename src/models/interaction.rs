use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::enums::QuestionType;

/// 视频互动问答点：在指定播放位置弹出的问题
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoQuestion {
    pub id: Uuid,
    pub video_id: Uuid,
    /// 问题触发的视频位置（秒）
    pub position_seconds: i32,
    pub question_type: QuestionType,
    pub content: String,
    /// 选项数组，JSON 格式，如 `[{"key":"A","text":"..."}]`
    pub options: Option<JsonValue>,
    /// 正确答案，JSON 格式
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateVideoQuestion {
    pub video_id: Uuid,
    pub position_seconds: i32,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateVideoQuestion {
    pub position_seconds: Option<i32>,
    pub content: Option<String>,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
}

// ---------------------------------------------------------------------------

/// 用户对视频问答点的回答记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoQuestionResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub question_id: Uuid,
    /// 用户提交的答案，JSON 格式
    pub answer: Option<JsonValue>,
    pub is_correct: Option<bool>,
    pub responded_at: DateTime<Utc>,
}

/// 用户提交视频问答回答时的输入
#[derive(Debug, Deserialize)]
pub struct SubmitVideoQuestionResponse {
    pub question_id: Uuid,
    pub answer: JsonValue,
}
