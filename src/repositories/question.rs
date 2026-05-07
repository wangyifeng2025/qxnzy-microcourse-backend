use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::models::enums::QuestionType;
use crate::models::question::{
    AddQuestionToExam, CreateQuestion, ExportedQuestion, Question, QuestionInExam, UpdateQuestion,
    UpdateQuestionInExam,
};
use crate::models::quiz::{CreateQuiz, Quiz, UpdateQuiz};

// ---------------------------------------------------------------------------
// 题库（Question Bank）
// ---------------------------------------------------------------------------

pub async fn create_question(
    pool: &PgPool,
    course_id: Uuid,
    created_by: Uuid,
    payload: &CreateQuestion,
) -> Result<Question, sqlx::Error> {
    let default_score = payload.default_score.unwrap_or(Decimal::from(5));
    sqlx::query_as!(
        Question,
        r#"
        INSERT INTO questions
            (course_id, chapter_id, video_id, created_by,
             question_type, content, options, correct_answer, explanation, default_score)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        RETURNING id, course_id, chapter_id, video_id, created_by,
                  question_type AS "question_type: _",
                  content, options, correct_answer, explanation, default_score,
                  created_at, updated_at
        "#,
        course_id,
        payload.chapter_id,
        payload.video_id,
        created_by,
        payload.question_type.clone() as QuestionType,
        payload.content,
        payload.options,
        payload.correct_answer,
        payload.explanation,
        default_score,
    )
    .fetch_one(pool)
    .await
}

pub async fn list_questions(
    pool: &PgPool,
    course_id: Uuid,
    chapter_id: Option<Uuid>,
    video_id: Option<Uuid>,
) -> Result<Vec<Question>, sqlx::Error> {
    sqlx::query_as!(
        Question,
        r#"
        SELECT id, course_id, chapter_id, video_id, created_by,
               question_type AS "question_type: _",
               content, options, correct_answer, explanation, default_score,
               created_at, updated_at
        FROM questions
        WHERE course_id = $1
          AND ($2::uuid IS NULL OR chapter_id = $2)
          AND ($3::uuid IS NULL OR video_id   = $3)
        ORDER BY created_at DESC
        "#,
        course_id,
        chapter_id,
        video_id,
    )
    .fetch_all(pool)
    .await
}

pub async fn find_question_by_id(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Question>, sqlx::Error> {
    sqlx::query_as!(
        Question,
        r#"
        SELECT id, course_id, chapter_id, video_id, created_by,
               question_type AS "question_type: _",
               content, options, correct_answer, explanation, default_score,
               created_at, updated_at
        FROM questions WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_question(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateQuestion,
) -> Result<Option<Question>, sqlx::Error> {
    sqlx::query_as!(
        Question,
        r#"
        UPDATE questions SET
            question_type  = $2,
            content        = $3,
            options        = $4,
            correct_answer = $5,
            explanation    = $6,
            default_score  = $7,
            chapter_id     = $8,
            video_id       = $9,
            updated_at     = NOW()
        WHERE id = $1
        RETURNING id, course_id, chapter_id, video_id, created_by,
                  question_type AS "question_type: _",
                  content, options, correct_answer, explanation, default_score,
                  created_at, updated_at
        "#,
        id,
        payload.question_type.clone() as QuestionType,
        payload.content,
        payload.options,
        payload.correct_answer,
        payload.explanation,
        payload.default_score,
        payload.chapter_id,
        payload.video_id,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete_question(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM questions WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

/// 批量导入题目（在同一事务中完成；chapter_id / video_id 均设为 null）
pub async fn import_questions_batch(
    pool: &PgPool,
    course_id: Uuid,
    created_by: Uuid,
    questions: &[ExportedQuestion],
) -> Result<Vec<Question>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let mut results = Vec::with_capacity(questions.len());

    for q in questions {
        let default_score = q.default_score;
        let row = sqlx::query_as!(
            Question,
            r#"
            INSERT INTO questions
                (course_id, chapter_id, video_id, created_by,
                 question_type, content, options, correct_answer, explanation, default_score)
            VALUES ($1, NULL, NULL, $2, $3, $4, $5, $6, $7, $8)
            RETURNING id, course_id, chapter_id, video_id, created_by,
                      question_type AS "question_type: _",
                      content, options, correct_answer, explanation, default_score,
                      created_at, updated_at
            "#,
            course_id,
            created_by,
            q.question_type.clone() as QuestionType,
            q.content,
            q.options,
            q.correct_answer,
            q.explanation,
            default_score,
        )
        .fetch_one(&mut *tx)
        .await?;

        results.push(row);
    }

    tx.commit().await?;
    Ok(results)
}

// ---------------------------------------------------------------------------
// 试卷（Exam / Quiz）CRUD
// ---------------------------------------------------------------------------

pub async fn create_exam(pool: &PgPool, payload: &CreateQuiz) -> Result<Quiz, sqlx::Error> {
    let total_score = payload.total_score.unwrap_or(Decimal::from(100));
    let pass_score = payload.pass_score.unwrap_or(Decimal::from(60));
    sqlx::query_as!(
        Quiz,
        r#"
        INSERT INTO quizzes
            (course_id, chapter_id, title, description, time_limit, total_score, pass_score, max_attempts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        RETURNING id, course_id, chapter_id, title, description, time_limit,
                  total_score, pass_score, max_attempts, is_published, created_at, updated_at
        "#,
        payload.course_id,
        payload.chapter_id,
        payload.title,
        payload.description,
        payload.time_limit,
        total_score,
        pass_score,
        payload.max_attempts,
    )
    .fetch_one(pool)
    .await
}

pub async fn list_exams(pool: &PgPool, course_id: Uuid) -> Result<Vec<Quiz>, sqlx::Error> {
    sqlx::query_as!(
        Quiz,
        r#"
        SELECT id, course_id, chapter_id, title, description, time_limit,
               total_score, pass_score, max_attempts, is_published, created_at, updated_at
        FROM quizzes
        WHERE course_id = $1
        ORDER BY created_at DESC
        "#,
        course_id
    )
    .fetch_all(pool)
    .await
}

pub async fn find_exam_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Quiz>, sqlx::Error> {
    sqlx::query_as!(
        Quiz,
        r#"
        SELECT id, course_id, chapter_id, title, description, time_limit,
               total_score, pass_score, max_attempts, is_published, created_at, updated_at
        FROM quizzes WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_exam(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateQuiz,
) -> Result<Option<Quiz>, sqlx::Error> {
    sqlx::query_as!(
        Quiz,
        r#"
        UPDATE quizzes SET
            title        = COALESCE($2, title),
            description  = $3,
            time_limit   = $4,
            total_score  = COALESCE($5, total_score),
            pass_score   = COALESCE($6, pass_score),
            max_attempts = $7,
            is_published = COALESCE($8, is_published),
            updated_at   = NOW()
        WHERE id = $1
        RETURNING id, course_id, chapter_id, title, description, time_limit,
                  total_score, pass_score, max_attempts, is_published, created_at, updated_at
        "#,
        id,
        payload.title,
        payload.description,
        payload.time_limit,
        payload.total_score,
        payload.pass_score,
        payload.max_attempts,
        payload.is_published,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete_exam(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM quizzes WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

pub async fn set_exam_published(
    pool: &PgPool,
    id: Uuid,
    published: bool,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE quizzes SET is_published = $2, updated_at = NOW() WHERE id = $1",
        id,
        published
    )
    .execute(pool)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 试卷组题（Exam Questions）
// ---------------------------------------------------------------------------

/// 查询试卷中所有题目（JOIN questions 表取完整字段）
pub async fn list_exam_questions(
    pool: &PgPool,
    quiz_id: Uuid,
) -> Result<Vec<QuestionInExam>, sqlx::Error> {
    sqlx::query_as::<_, QuestionInExam>(
        r#"
        SELECT
            eq.id          AS entry_id,
            eq.quiz_id,
            eq.question_id,
            eq.score,
            eq.sort_order,
            eq.created_at  AS entry_created_at,
            q.question_type,
            q.content,
            q.options,
            q.correct_answer,
            q.explanation,
            q.default_score,
            q.chapter_id,
            q.video_id
        FROM exam_questions eq
        JOIN questions q ON q.id = eq.question_id
        WHERE eq.quiz_id = $1
        ORDER BY eq.sort_order ASC, eq.created_at ASC
        "#,
    )
    .bind(quiz_id)
    .fetch_all(pool)
    .await
}

/// 将题库题目加入试卷（若已存在则更新分值和顺序）
pub async fn add_question_to_exam(
    pool: &PgPool,
    quiz_id: Uuid,
    payload: &AddQuestionToExam,
) -> Result<QuestionInExam, sqlx::Error> {
    let sort_order = payload.sort_order.unwrap_or(0);

    let entry_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO exam_questions (quiz_id, question_id, score, sort_order)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (quiz_id, question_id) DO UPDATE
            SET score      = EXCLUDED.score,
                sort_order = EXCLUDED.sort_order
        RETURNING id
        "#,
        quiz_id,
        payload.question_id,
        payload.score,
        sort_order,
    )
    .fetch_one(pool)
    .await?;

    find_exam_question_by_entry_id(pool, entry_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 更新试卷中某道题的分值 / 排序（只更新有值的字段）
pub async fn update_question_in_exam(
    pool: &PgPool,
    entry_id: Uuid,
    payload: &UpdateQuestionInExam,
) -> Result<Option<QuestionInExam>, sqlx::Error> {
    let updated = sqlx::query_scalar!(
        r#"
        UPDATE exam_questions
        SET score      = COALESCE($2, score),
            sort_order = COALESCE($3, sort_order)
        WHERE id = $1
        RETURNING id
        "#,
        entry_id,
        payload.score,
        payload.sort_order,
    )
    .fetch_optional(pool)
    .await?;

    match updated {
        Some(id) => find_exam_question_by_entry_id(pool, id).await,
        None => Ok(None),
    }
}

/// 从试卷中移除一道题
pub async fn remove_question_from_exam(
    pool: &PgPool,
    entry_id: Uuid,
) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM exam_questions WHERE id = $1", entry_id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

/// 内部辅助：按 entry_id 取组题记录（含题目详情）
async fn find_exam_question_by_entry_id(
    pool: &PgPool,
    entry_id: Uuid,
) -> Result<Option<QuestionInExam>, sqlx::Error> {
    sqlx::query_as::<_, QuestionInExam>(
        r#"
        SELECT
            eq.id          AS entry_id,
            eq.quiz_id,
            eq.question_id,
            eq.score,
            eq.sort_order,
            eq.created_at  AS entry_created_at,
            q.question_type,
            q.content,
            q.options,
            q.correct_answer,
            q.explanation,
            q.default_score,
            q.chapter_id,
            q.video_id
        FROM exam_questions eq
        JOIN questions q ON q.id = eq.question_id
        WHERE eq.id = $1
        "#,
    )
    .bind(entry_id)
    .fetch_optional(pool)
    .await
}
