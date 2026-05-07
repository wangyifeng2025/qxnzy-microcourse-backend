use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::quiz::QuizAttempt;

// ---------------------------------------------------------------------------
// 答题记录 CRUD
// ---------------------------------------------------------------------------

/// 开始一次答题（INSERT 新记录）。
/// 调用方应在此之前已校验最大次数限制（count_submitted_attempts）。
pub async fn start_attempt(
    pool: &PgPool,
    user_id: Uuid,
    quiz_id: Uuid,
) -> Result<QuizAttempt, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        INSERT INTO quiz_attempts (user_id, quiz_id)
        VALUES ($1, $2)
        RETURNING id, user_id, quiz_id, score, answers, question_scores,
                  is_graded, started_at, submitted_at, time_spent
        "#,
        user_id,
        quiz_id,
    )
    .fetch_one(pool)
    .await
}

pub async fn find_attempt(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<QuizAttempt>, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        SELECT id, user_id, quiz_id, score, answers, question_scores,
               is_graded, started_at, submitted_at, time_spent
        FROM quiz_attempts WHERE id = $1
        "#,
        id,
    )
    .fetch_optional(pool)
    .await
}

/// 查询某试卷的所有答题记录（教师视角）
pub async fn list_by_quiz(
    pool: &PgPool,
    quiz_id: Uuid,
) -> Result<Vec<QuizAttempt>, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        SELECT id, user_id, quiz_id, score, answers, question_scores,
               is_graded, started_at, submitted_at, time_spent
        FROM quiz_attempts
        WHERE quiz_id = $1
        ORDER BY started_at DESC
        "#,
        quiz_id,
    )
    .fetch_all(pool)
    .await
}

/// 查询某学生在某试卷的答题历史（学生视角）
pub async fn list_by_user(
    pool: &PgPool,
    user_id: Uuid,
    quiz_id: Uuid,
) -> Result<Vec<QuizAttempt>, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        SELECT id, user_id, quiz_id, score, answers, question_scores,
               is_graded, started_at, submitted_at, time_spent
        FROM quiz_attempts
        WHERE user_id = $1 AND quiz_id = $2
        ORDER BY started_at DESC
        "#,
        user_id,
        quiz_id,
    )
    .fetch_all(pool)
    .await
}

/// 统计某学生在某试卷已提交的次数（用于校验 max_attempts）
pub async fn count_submitted_attempts(
    pool: &PgPool,
    user_id: Uuid,
    quiz_id: Uuid,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) AS "count!"
        FROM quiz_attempts
        WHERE user_id = $1 AND quiz_id = $2 AND submitted_at IS NOT NULL
        "#,
        user_id,
        quiz_id,
    )
    .fetch_one(pool)
    .await
}

/// 提交答卷：写入答案、每题得分、总分、是否已评分、耗时
pub async fn submit_attempt(
    pool: &PgPool,
    attempt_id: Uuid,
    answers: &JsonValue,
    question_scores: &JsonValue,
    total_score: Decimal,
    is_graded: bool,
    time_spent: Option<i32>,
) -> Result<QuizAttempt, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        UPDATE quiz_attempts SET
            answers         = $2,
            question_scores = $3,
            score           = $4,
            is_graded       = $5,
            time_spent      = $6,
            submitted_at    = NOW()
        WHERE id = $1
        RETURNING id, user_id, quiz_id, score, answers, question_scores,
                  is_graded, started_at, submitted_at, time_spent
        "#,
        attempt_id,
        answers,
        question_scores,
        total_score,
        is_graded,
        time_spent,
    )
    .fetch_one(pool)
    .await
}

/// 教师批改：更新 question_scores 中的问答题得分并重算总分；全部批改后 is_graded = true
pub async fn update_grades(
    pool: &PgPool,
    attempt_id: Uuid,
    new_question_scores: &JsonValue,
    new_total_score: Decimal,
    all_graded: bool,
) -> Result<QuizAttempt, sqlx::Error> {
    sqlx::query_as!(
        QuizAttempt,
        r#"
        UPDATE quiz_attempts SET
            question_scores = $2,
            score           = $3,
            is_graded       = $4
        WHERE id = $1
        RETURNING id, user_id, quiz_id, score, answers, question_scores,
                  is_graded, started_at, submitted_at, time_spent
        "#,
        attempt_id,
        new_question_scores,
        new_total_score,
        all_graded,
    )
    .fetch_one(pool)
    .await
}
