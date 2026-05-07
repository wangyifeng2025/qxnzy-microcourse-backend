use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use uuid::Uuid;

use super::enums::QuestionType;
use super::quiz::Quiz;

// ---------------------------------------------------------------------------
// 题库（Question Bank）
// ---------------------------------------------------------------------------

/// 题库中的一道独立题目
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Question {
    pub id: Uuid,
    pub course_id: Uuid,
    /// 所属章节（可选，用于按章节筛选）
    pub chapter_id: Option<Uuid>,
    /// 所属小节视频（可选，最细粒度）
    pub video_id: Option<Uuid>,
    pub created_by: Uuid,
    pub question_type: QuestionType,
    pub content: String,
    /// 选项（选择题）：[{"key":"A","text":"..."},...]
    pub options: Option<JsonValue>,
    /// 正确答案（选择题 / 判断题自动评分用）；问答题为 null
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub default_score: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 创建题目的请求体
#[derive(Debug, Deserialize)]
pub struct CreateQuestion {
    pub chapter_id: Option<Uuid>,
    pub video_id: Option<Uuid>,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub default_score: Option<Decimal>,
}

/// 更新题目的请求体（PUT 语义：所有字段均可修改；可空字段发 null 表示清除）
#[derive(Debug, Deserialize)]
pub struct UpdateQuestion {
    pub chapter_id: Option<Uuid>,
    pub video_id: Option<Uuid>,
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub default_score: Decimal,
}

/// 题目列表查询参数
#[derive(Debug, Deserialize)]
pub struct ListQuestionsParams {
    pub chapter_id: Option<Uuid>,
    pub video_id: Option<Uuid>,
}

// ---------------------------------------------------------------------------
// 试卷组题（Exam Composition）
// ---------------------------------------------------------------------------

/// exam_questions 行 + JOIN questions 得到的完整字段（用于试卷详情展示）
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct QuestionInExam {
    /// exam_questions.id（组题记录主键）
    pub entry_id: Uuid,
    pub quiz_id: Uuid,
    pub question_id: Uuid,
    /// 该题在本试卷中的自定义分值；null 时应沿用 default_score
    pub score: Option<Decimal>,
    pub sort_order: i32,
    pub entry_created_at: DateTime<Utc>,
    // 来自 questions 表的字段
    pub question_type: QuestionType,
    pub content: String,
    pub options: Option<JsonValue>,
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub default_score: Decimal,
    pub chapter_id: Option<Uuid>,
    pub video_id: Option<Uuid>,
}

impl QuestionInExam {
    /// 该题在试卷中实际生效的分值（优先用自定义分值，回退到默认分值）
    pub fn effective_score(&self) -> Decimal {
        self.score.unwrap_or(self.default_score)
    }
}

/// 试卷详情：元信息 + 题目列表
#[derive(Debug, Serialize)]
pub struct ExamDetail {
    #[serde(flatten)]
    pub exam: Quiz,
    pub questions: Vec<QuestionInExam>,
    /// 所有题目有效分值之和
    pub computed_total_score: Decimal,
}

/// 向试卷添加题目的请求体
#[derive(Debug, Deserialize)]
pub struct AddQuestionToExam {
    pub question_id: Uuid,
    /// 本试卷中该题的自定义分值；不填则沿用 questions.default_score
    pub score: Option<Decimal>,
    pub sort_order: Option<i32>,
}

/// 更新试卷中某道题配置的请求体
#[derive(Debug, Deserialize)]
pub struct UpdateQuestionInExam {
    pub score: Option<Decimal>,
    pub sort_order: Option<i32>,
}

// ---------------------------------------------------------------------------
// 题库导入 / 导出
// ---------------------------------------------------------------------------

/// 导出文件中单道题目的数据形状（只含可移植字段，无服务端生成字段）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportedQuestion {
    pub question_type: QuestionType,
    pub content: String,
    /// 选择题选项；其他题型为 null
    pub options: Option<JsonValue>,
    /// 客观题正确答案；问答题为 null
    pub correct_answer: Option<JsonValue>,
    pub explanation: Option<String>,
    pub default_score: Decimal,
    /// 原始章节 ID（仅供参考，导入时自动忽略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter_id: Option<Uuid>,
    /// 原始视频 ID（仅供参考，导入时自动忽略）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_id: Option<Uuid>,
}

/// 单条导入错误信息
#[derive(Debug, Serialize)]
pub struct ImportError {
    /// 从 1 开始的题目序号
    pub index: usize,
    pub message: String,
}

/// 导入操作结果摘要
#[derive(Debug, Serialize)]
pub struct ImportResult {
    /// 请求中题目总数
    pub total: usize,
    /// 成功导入数量
    pub imported: usize,
    /// 跳过/失败数量
    pub failed: usize,
    pub errors: Vec<ImportError>,
}
