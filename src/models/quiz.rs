use std::collections::HashMap;

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
// 答题记录
// ---------------------------------------------------------------------------

/// 答题记录（与 quiz_attempts 表对应）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuizAttempt {
    pub id: Uuid,
    pub user_id: Uuid,
    pub quiz_id: Uuid,
    /// NULL 表示未评分
    pub score: Option<Decimal>,
    /// 用户提交的所有答案：`{question_id: answer_value}`
    pub answers: Option<JsonValue>,
    /// 每题得分：`{question_id: score_or_null}`，null 表示问答题尚未批改
    pub question_scores: Option<JsonValue>,
    pub is_graded: bool,
    pub started_at: DateTime<Utc>,
    pub submitted_at: Option<DateTime<Utc>>,
    /// 答题耗时（秒）
    pub time_spent: Option<i32>,
}

/// 学生提交答卷时的请求体
#[derive(Debug, Deserialize)]
pub struct SubmitAnswers {
    /// 答案 Map：`{question_id_str: answer_value}`
    /// answer_value 可以是字符串（单选）、字符串数组（多选）、布尔（判断）、字符串（问答）
    pub answers: JsonValue,
    /// 答题耗时（秒），由前端计时
    pub time_spent: Option<i32>,
}

/// 教师批改问答题时的请求体
#[derive(Debug, Deserialize)]
pub struct GradeEssayPayload {
    /// `{question_id: score}` — 仅列出本次要批改的问答题
    pub essay_scores: HashMap<String, Decimal>,
}

// ---------------------------------------------------------------------------
// 答题详情视图（仅在提交后可见完整评分与解析）
// ---------------------------------------------------------------------------

/// 单题答题结果
#[derive(Debug, Serialize)]
pub struct AttemptQuestionResult {
    pub question_id: Uuid,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    /// 该题在本试卷中的有效分值
    pub effective_score: Decimal,
    /// 学生答案（提交前为 null）
    pub student_answer: Option<JsonValue>,
    /// 该题得分（提交后可见；问答题批改前为 null）
    pub earned_score: Option<Decimal>,
    /// 是否正确（仅客观题有值；问答题为 null）
    pub is_correct: Option<bool>,
    /// 正确答案（提交后可见）
    pub correct_answer: Option<JsonValue>,
    /// 解析（提交后可见）
    pub explanation: Option<String>,
}

/// 完整答题详情（含试卷元信息 + 每题结果）
#[derive(Debug, Serialize)]
pub struct AttemptDetail {
    #[serde(flatten)]
    pub attempt: QuizAttempt,
    pub questions: Vec<AttemptQuestionResult>,
}

/// 开始答题后返回的数据（含题目列表，**不含正确答案**）
#[derive(Debug, Serialize)]
pub struct StartAttemptResponse {
    pub attempt_id: Uuid,
    pub quiz_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub time_limit: Option<i32>,
    pub total_score: Decimal,
    pub pass_score: Decimal,
    /// 题目列表，correct_answer 全部置为 null
    pub questions: Vec<ExamQuestionForStudent>,
}

/// 学生答题时看到的题目（不含正确答案）
#[derive(Debug, Serialize)]
pub struct ExamQuestionForStudent {
    pub question_id: Uuid,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    /// 该题在本试卷中的有效分值
    pub score: Decimal,
    pub sort_order: i32,
}
