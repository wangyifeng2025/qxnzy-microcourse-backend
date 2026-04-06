use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::enums::QuestionType;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Quiz {
    pub id: Uuid,
    pub course_id: Uuid,
    /// NULL 表示课程级测试，非 NULL 表示章节测试
    pub chapter_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    /// 限时（分钟），NULL 不限时
    pub time_limit: Option<i32>,
    pub total_score: Decimal,
    pub pass_score: Decimal,
    /// 最大尝试次数，NULL 不限次
    pub max_attempts: Option<i32>,
    pub is_published: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuiz {
    pub course_id: Uuid,
    pub chapter_id: Option<Uuid>,
    pub title: String,
    pub description: Option<String>,
    pub time_limit: Option<i32>,
    pub total_score: Option<Decimal>,
    pub pass_score: Option<Decimal>,
    pub max_attempts: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateQuiz {
    pub title: Option<String>,
    pub description: Option<String>,
    pub time_limit: Option<i32>,
    pub total_score: Option<Decimal>,
    pub pass_score: Option<Decimal>,
    pub max_attempts: Option<i32>,
    pub is_published: Option<bool>,
}

// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuizQuestion {
    pub id: Uuid,
    pub quiz_id: Uuid,
    pub question_type: QuestionType,
    pub content: String,
    /// 选项数组，JSON 格式，如 `[{"key":"A","text":"..."}]`
    pub options: Option<JsonValue>,
    /// 正确答案，JSON 格式
    pub correct_answer: Option<JsonValue>,
    pub score: Option<Decimal>,
    pub explanation: Option<String>,
    pub sort_order: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateQuizQuestion {
    pub quiz_id: Uuid,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub score: Option<Decimal>,
    pub explanation: Option<String>,
    pub sort_order: Option<i32>,
}

// ---------------------------------------------------------------------------

/// 答题记录
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuizAttempt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub quiz_id: Uuid,
    /// NULL 表示未评分
    pub score: Option<Decimal>,
    /// 用户提交的所有答案，JSON 格式
    pub answers: Option<JsonValue>,
    pub is_graded: bool,
    pub started_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    /// 答题耗时（秒）
    pub time_spent: Option<i32>,
}

/// 提交答卷时的输入
#[derive(Debug, Deserialize)]
pub struct SubmitQuizAttempt {
    pub quiz_id: Uuid,
    pub answers: JsonValue,
    /// 答题耗时（秒）
    pub time_spent: Option<i32>,
}
