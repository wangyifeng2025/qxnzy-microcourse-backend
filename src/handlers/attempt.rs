use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    Json,
};
use rust_decimal::Decimal;
use serde_json::Value as JsonValue;
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::enums::{QuestionType, UserRole};
use crate::models::question::QuestionInExam;
use crate::models::quiz::{
    AttemptDetail, AttemptQuestionResult, ExamQuestionForStudent, GradeEssayPayload,
    QuizAttempt, StartAttemptResponse, SubmitAnswers,
};
use crate::repositories::{
    attempt as attempt_repo, course as course_repo, enrollment as enrollment_repo,
    question as question_repo,
};

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("{}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn bad_request(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

fn forbidden(msg: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, msg.to_string())
}

// ---------------------------------------------------------------------------
// 权限辅助
// ---------------------------------------------------------------------------

/// 确认用户已选课（或是教师 / 管理员）
async fn ensure_enrolled_or_teacher(
    pool: &PgPool,
    auth: &AuthContext,
    course_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if matches!(auth.role, UserRole::Admin | UserRole::Teacher) {
        return Ok(());
    }
    let enrolled = enrollment_repo::find_by_user_and_course(pool, auth.user_id, course_id)
        .await
        .map_err(internal_error)?;
    if enrolled.is_none() {
        return Err(forbidden("请先选课才能参加测试"));
    }
    Ok(())
}

/// 确认试卷属于该课程且已发布（学生视角）
async fn get_published_exam(
    pool: &PgPool,
    course_id: Uuid,
    exam_id: Uuid,
) -> Result<crate::models::quiz::Quiz, (StatusCode, String)> {
    let exam = question_repo::find_exam_by_id(pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if exam.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }
    if !exam.is_published {
        return Err(not_found("试卷不存在或尚未发布"));
    }
    Ok(exam)
}

/// 校验 attempt 属于该试卷，且当前用户有权访问（学生只能看自己的；教师可看全部）
async fn get_attempt_with_auth(
    pool: &PgPool,
    auth: &AuthContext,
    exam_id: Uuid,
    attempt_id: Uuid,
) -> Result<QuizAttempt, (StatusCode, String)> {
    let attempt = attempt_repo::find_attempt(pool, attempt_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("答题记录不存在"))?;

    if attempt.quiz_id != exam_id {
        return Err(not_found("答题记录不属于该试卷"));
    }

    // 学生只能查看自己的答题记录
    if auth.role == UserRole::Student && attempt.user_id != auth.user_id {
        return Err(forbidden("无权查看他人的答题记录"));
    }

    Ok(attempt)
}

// ---------------------------------------------------------------------------
// 自动评分
// ---------------------------------------------------------------------------

/// 对客观题自动评分，问答题记 null。
/// 返回 (question_scores JsonValue, auto_total Decimal, has_ungraded bool)
fn auto_grade(
    answers: &JsonValue,
    exam_questions: &[QuestionInExam],
) -> (JsonValue, Decimal, bool) {
    use serde_json::Map;

    let mut scores_map = Map::new();
    let mut total = Decimal::ZERO;
    let mut has_ungraded = false;

    for q in exam_questions {
        let qid = q.question_id.to_string();
        let effective = q.effective_score();
        let student_ans = answers.get(&qid);

        match q.question_type {
            QuestionType::Essay => {
                // 问答题由教师批改，暂记 null
                scores_map.insert(qid, JsonValue::Null);
                has_ungraded = true;
            }
            _ => {
                let correct = q.correct_answer.as_ref();
                let earned = match (student_ans, correct) {
                    (Some(sa), Some(ca)) if answers_match(sa, ca, &q.question_type) => {
                        total += effective;
                        JsonValue::from(
                            effective.to_string().parse::<f64>().unwrap_or(0.0),
                        )
                    }
                    _ => JsonValue::from(0.0_f64),
                };
                scores_map.insert(qid, earned);
            }
        }
    }

    (JsonValue::Object(scores_map), total, has_ungraded)
}

/// 答案比对（客观题）
fn answers_match(student: &JsonValue, correct: &JsonValue, qt: &QuestionType) -> bool {
    match qt {
        QuestionType::SingleChoice => {
            // 字符串相等（忽略大小写）
            let s = student.as_str().unwrap_or("").to_uppercase();
            let c = correct.as_str().unwrap_or("").to_uppercase();
            !s.is_empty() && s == c
        }
        QuestionType::MultipleChoice => {
            // 数组元素相同（顺序无关）
            let mut sa = student
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_uppercase))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let mut ca = correct
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(str::to_uppercase))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            sa.sort();
            ca.sort();
            !sa.is_empty() && sa == ca
        }
        QuestionType::TrueFalse => {
            // 布尔值相等
            student.as_bool() == correct.as_bool() && correct.as_bool().is_some()
        }
        QuestionType::Essay => false, // 不自动评分
    }
}

/// 从 question_scores JsonValue 中汇总总分（跳过 null）
fn sum_question_scores(question_scores: &JsonValue) -> Decimal {
    let mut total = Decimal::ZERO;
    if let Some(map) = question_scores.as_object() {
        for v in map.values() {
            if let Some(f) = v.as_f64() {
                if let Ok(d) = Decimal::try_from(f) {
                    total += d;
                }
            }
        }
    }
    total
}

/// 判断 question_scores 中是否还有 null（即是否还有未批改的问答题）
fn has_null_scores(question_scores: &JsonValue) -> bool {
    if let Some(map) = question_scores.as_object() {
        return map.values().any(|v| v.is_null());
    }
    false
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// POST /api/courses/{course_id}/exams/{exam_id}/attempts
///
/// 学生开始答题。返回题目列表（不含正确答案）和 attempt_id。
/// - 校验试卷已发布
/// - 校验已选课
/// - 校验未超过最大答题次数
/// - 校验没有进行中（未提交）的答题
pub async fn start_attempt(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> Result<(StatusCode, Json<StartAttemptResponse>), (StatusCode, String)> {
    ensure_enrolled_or_teacher(&pool, &auth, course_id).await?;
    let exam = get_published_exam(&pool, course_id, exam_id).await?;

    // 检查最大答题次数
    if let Some(max) = exam.max_attempts {
        let submitted =
            attempt_repo::count_submitted_attempts(&pool, auth.user_id, exam_id)
                .await
                .map_err(internal_error)?;
        if submitted >= max as i64 {
            return Err(bad_request(&format!(
                "已达最大答题次数限制（{max} 次），无法再次作答"
            )));
        }
    }

    // 检查是否有进行中的答题（submitted_at IS NULL）
    let my_attempts = attempt_repo::list_by_user(&pool, auth.user_id, exam_id)
        .await
        .map_err(internal_error)?;
    let in_progress = my_attempts.iter().find(|a| a.submitted_at.is_none());
    if in_progress.is_some() {
        return Err(bad_request("您有一次未完成的答题，请先提交或等待超时"));
    }

    // 创建新答题记录
    let attempt = attempt_repo::start_attempt(&pool, auth.user_id, exam_id)
        .await
        .map_err(internal_error)?;

    // 取试卷题目，去掉正确答案
    let exam_questions = question_repo::list_exam_questions(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    let questions: Vec<ExamQuestionForStudent> = exam_questions
        .into_iter()
        .map(|q| {
            let score = q.effective_score();
            ExamQuestionForStudent {
                question_id: q.question_id,
                question_type: q.question_type,
                content: q.content,
                options: q.options,
                score,
                sort_order: q.sort_order,
            }
        })
        .collect();

    Ok((
        StatusCode::CREATED,
        Json(StartAttemptResponse {
            attempt_id: attempt.id,
            quiz_id: exam_id,
            title: exam.title,
            description: exam.description,
            time_limit: exam.time_limit,
            total_score: exam.total_score,
            pass_score: exam.pass_score,
            questions,
        }),
    ))
}

/// POST /api/courses/{course_id}/exams/{exam_id}/attempts/{attempt_id}/submit
///
/// 学生提交答案。触发自动评分，问答题留 null 等待教师批改。
pub async fn submit_attempt(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<SubmitAnswers>,
) -> AppResult<QuizAttempt> {
    ensure_enrolled_or_teacher(&pool, &auth, course_id).await?;

    let attempt = get_attempt_with_auth(&pool, &auth, exam_id, attempt_id).await?;

    if attempt.submitted_at.is_some() {
        return Err(bad_request("该答题记录已提交，不能重复提交"));
    }

    // 取试卷题目做评分
    let exam_questions = question_repo::list_exam_questions(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    let (question_scores, auto_total, has_ungraded) =
        auto_grade(&payload.answers, &exam_questions);

    let is_graded = !has_ungraded;

    let updated = attempt_repo::submit_attempt(
        &pool,
        attempt_id,
        &payload.answers,
        &question_scores,
        auto_total,
        is_graded,
        payload.time_spent,
    )
    .await
    .map_err(internal_error)?;

    Ok(Json(updated))
}

/// GET /api/courses/{course_id}/exams/{exam_id}/attempts/my
///
/// 学生查看自己在该试卷的所有答题记录。
pub async fn get_my_attempts(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Vec<QuizAttempt>> {
    ensure_enrolled_or_teacher(&pool, &auth, course_id).await?;
    get_published_exam(&pool, course_id, exam_id).await?;

    let attempts = attempt_repo::list_by_user(&pool, auth.user_id, exam_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(attempts))
}

/// GET /api/courses/{course_id}/exams/{exam_id}/attempts/{attempt_id}
///
/// 查看答题详情（含每题评分和解析）。
/// - 已提交才能看到正确答案和解析
/// - 学生只能看自己的；教师可看全部
pub async fn get_attempt_detail(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
) -> AppResult<AttemptDetail> {
    ensure_enrolled_or_teacher(&pool, &auth, course_id).await?;

    let attempt = get_attempt_with_auth(&pool, &auth, exam_id, attempt_id).await?;
    let submitted = attempt.submitted_at.is_some();

    let exam_questions = question_repo::list_exam_questions(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    let answers = attempt.answers.as_ref().cloned().unwrap_or(JsonValue::Null);
    let qs_map = attempt.question_scores.as_ref().cloned().unwrap_or(JsonValue::Null);

    let questions: Vec<AttemptQuestionResult> = exam_questions
        .into_iter()
        .map(|q| {
            let qid = q.question_id.to_string();
            let student_answer = if submitted {
                answers.get(&qid).cloned()
            } else {
                None
            };
            let earned_score: Option<Decimal> = if submitted {
                qs_map.get(&qid).and_then(|v| {
                    v.as_f64()
                        .and_then(|f| Decimal::try_from(f).ok())
                })
            } else {
                None
            };
            let is_correct = if submitted {
                match q.question_type {
                    QuestionType::Essay => None,
                    _ => earned_score.map(|s| s > Decimal::ZERO),
                }
            } else {
                None
            };
            let effective = q.effective_score();
            AttemptQuestionResult {
                question_id: q.question_id,
                question_type: q.question_type,
                content: q.content,
                options: q.options,
                effective_score: effective,
                student_answer,
                earned_score,
                is_correct,
                correct_answer: if submitted { q.correct_answer } else { None },
                explanation: if submitted { q.explanation } else { None },
            }
        })
        .collect();

    Ok(Json(AttemptDetail { attempt, questions }))
}

/// GET /api/courses/{course_id}/exams/{exam_id}/attempts
///
/// 教师查看所有学生的答题记录列表。
pub async fn list_all_attempts(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Vec<QuizAttempt>> {
    // 只有教师/管理员可以查看所有人的记录
    if auth.role == UserRole::Student {
        return Err(forbidden("仅教师或管理员可查看全部答题记录"));
    }

    // 验证课程所有权
    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if auth.role != UserRole::Admin && course.teacher_id != auth.user_id {
        return Err(forbidden("仅课程教师或管理员可查看"));
    }

    let exam = question_repo::find_exam_by_id(&pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if exam.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }

    let attempts = attempt_repo::list_by_quiz(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(attempts))
}

/// PUT /api/courses/{course_id}/exams/{exam_id}/attempts/{attempt_id}/grade
///
/// 教师批改问答题：更新每题得分并重算总分。
/// 若所有问答题均已批改则将 is_graded 置为 true。
pub async fn grade_attempt(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id, attempt_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<GradeEssayPayload>,
) -> AppResult<QuizAttempt> {
    // 仅教师/管理员可批改
    if auth.role == UserRole::Student {
        return Err(forbidden("仅教师或管理员可批改问答题"));
    }

    let course = course_repo::find_by_id(&pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if auth.role != UserRole::Admin && course.teacher_id != auth.user_id {
        return Err(forbidden("仅课程教师或管理员可批改"));
    }

    let attempt = attempt_repo::find_attempt(&pool, attempt_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("答题记录不存在"))?;

    if attempt.quiz_id != exam_id {
        return Err(not_found("答题记录不属于该试卷"));
    }

    if attempt.submitted_at.is_none() {
        return Err(bad_request("该答题记录尚未提交，不能批改"));
    }

    // 将教师提供的问答题得分合并到现有 question_scores 中
    let mut qs_map = attempt
        .question_scores
        .clone()
        .unwrap_or_else(|| JsonValue::Object(Default::default()));

    let map = qs_map.as_object_mut().ok_or_else(|| {
        internal_error("question_scores 格式异常")
    })?;

    for (qid_str, score) in &payload.essay_scores {
        let f: f64 = score.to_string().parse().unwrap_or(0.0);
        map.insert(qid_str.clone(), JsonValue::from(f));
    }

    let new_total = sum_question_scores(&qs_map);
    let all_graded = !has_null_scores(&qs_map);

    let updated =
        attempt_repo::update_grades(&pool, attempt_id, &qs_map, new_total, all_graded)
            .await
            .map_err(internal_error)?;

    Ok(Json(updated))
}
