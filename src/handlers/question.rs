use axum::{
    extract::{Extension, Multipart, Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::enums::{QuestionType, UserRole};
use crate::models::question::{
    AddQuestionToExam, CreateQuestion, ExamDetail, ExportedQuestion, ImportError, ImportResult,
    ListQuestionsParams, Question, QuestionInExam, UpdateQuestion, UpdateQuestionInExam,
};
use crate::models::quiz::{CreateQuiz, Quiz, UpdateQuiz};
use crate::repositories::{course as course_repo, enrollment as enrollment_repo, question as question_repo};

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

/// 校验当前用户是否为该课程的教师或管理员
async fn ensure_course_teacher(
    pool: &PgPool,
    auth: &AuthContext,
    course_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin {
        return Ok(());
    }
    let course = course_repo::find_by_id(pool, course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;
    if course.teacher_id != auth.user_id {
        return Err(forbidden("仅课程教师或管理员可操作"));
    }
    Ok(())
}

/// 校验当前用户是否拥有该试卷所属课程的权限，并返回试卷
async fn ensure_exam_owner(
    pool: &PgPool,
    auth: &AuthContext,
    exam_id: Uuid,
) -> Result<Quiz, (StatusCode, String)> {
    let exam = question_repo::find_exam_by_id(pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if auth.role == UserRole::Admin {
        return Ok(exam);
    }

    let course = course_repo::find_by_id(pool, exam.course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if course.teacher_id != auth.user_id {
        return Err(forbidden("仅课程教师或管理员可操作"));
    }
    Ok(exam)
}

// ---------------------------------------------------------------------------
// 题库管理
// ---------------------------------------------------------------------------

/// GET /api/courses/{course_id}/questions?chapter_id=&video_id=
pub async fn list_questions(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Query(params): Query<ListQuestionsParams>,
) -> AppResult<Vec<Question>> {
    ensure_course_teacher(&pool, &auth, course_id).await?;
    let questions = question_repo::list_questions(&pool, course_id, params.chapter_id, params.video_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(questions))
}

/// POST /api/courses/{course_id}/questions
pub async fn create_question(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Json(payload): Json<CreateQuestion>,
) -> Result<(StatusCode, Json<Question>), (StatusCode, String)> {
    if payload.content.trim().is_empty() {
        return Err(bad_request("题目内容不能为空"));
    }
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let question = question_repo::create_question(&pool, course_id, auth.user_id, &payload)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(question)))
}

/// GET /api/courses/{course_id}/questions/{question_id}
pub async fn get_question(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, question_id)): Path<(Uuid, Uuid)>,
) -> AppResult<Question> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let question = question_repo::find_question_by_id(&pool, question_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("题目不存在"))?;

    if question.course_id != course_id {
        return Err(not_found("题目不属于该课程"));
    }

    Ok(Json(question))
}

/// PUT /api/courses/{course_id}/questions/{question_id}
pub async fn update_question(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, question_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateQuestion>,
) -> AppResult<Question> {
    if payload.content.trim().is_empty() {
        return Err(bad_request("题目内容不能为空"));
    }
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let existing = question_repo::find_question_by_id(&pool, question_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("题目不存在"))?;

    if existing.course_id != course_id {
        return Err(not_found("题目不属于该课程"));
    }

    let updated = question_repo::update_question(&pool, question_id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("题目不存在"))?;

    Ok(Json(updated))
}

/// DELETE /api/courses/{course_id}/questions/{question_id}
pub async fn delete_question(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, question_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let existing = question_repo::find_question_by_id(&pool, question_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("题目不存在"))?;

    if existing.course_id != course_id {
        return Err(not_found("题目不属于该课程"));
    }

    question_repo::delete_question(&pool, question_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 试卷管理
// ---------------------------------------------------------------------------

/// GET /api/courses/{course_id}/exams
///
/// - 教师/管理员：返回所有试卷（含未发布）
/// - 已选课学生：只返回已发布试卷
pub async fn list_exams(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
) -> AppResult<Vec<Quiz>> {
    let is_teacher = matches!(auth.role, UserRole::Admin | UserRole::Teacher);

    if is_teacher {
        // 教师校验课程所有权
        ensure_course_teacher(&pool, &auth, course_id).await?;
        let exams = question_repo::list_exams(&pool, course_id)
            .await
            .map_err(internal_error)?;
        Ok(Json(exams))
    } else {
        // 学生校验选课状态
        let enrolled = enrollment_repo::find_by_user_and_course(&pool, auth.user_id, course_id)
            .await
            .map_err(internal_error)?;
        if enrolled.is_none() {
            return Err(forbidden("请先选课才能查看测试"));
        }
        let all = question_repo::list_exams(&pool, course_id)
            .await
            .map_err(internal_error)?;
        let published: Vec<Quiz> = all.into_iter().filter(|e| e.is_published).collect();
        Ok(Json(published))
    }
}

/// POST /api/courses/{course_id}/exams
pub async fn create_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Json(mut payload): Json<CreateQuiz>,
) -> Result<(StatusCode, Json<Quiz>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("试卷标题不能为空"));
    }
    ensure_course_teacher(&pool, &auth, course_id).await?;

    // 确保 course_id 与路径一致（防止请求体中伪造 course_id）
    payload.course_id = course_id;

    let exam = question_repo::create_exam(&pool, &payload)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(exam)))
}

/// GET /api/courses/{course_id}/exams/{exam_id}
///
/// - 教师/管理员：完整返回（含 correct_answer）
/// - 已选课学生：只能看已发布试卷，**correct_answer 置为 null**
pub async fn get_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> AppResult<ExamDetail> {
    let is_teacher = matches!(auth.role, UserRole::Admin | UserRole::Teacher);

    if is_teacher {
        ensure_course_teacher(&pool, &auth, course_id).await?;
    } else {
        let enrolled = enrollment_repo::find_by_user_and_course(&pool, auth.user_id, course_id)
            .await
            .map_err(internal_error)?;
        if enrolled.is_none() {
            return Err(forbidden("请先选课才能查看测试"));
        }
    }

    let exam = question_repo::find_exam_by_id(&pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if exam.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }

    // 学生只能看已发布的试卷
    if !is_teacher && !exam.is_published {
        return Err(not_found("试卷不存在或尚未发布"));
    }

    let mut questions = question_repo::list_exam_questions(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    // 学生不能看到正确答案
    if !is_teacher {
        for q in &mut questions {
            q.correct_answer = None;
            q.explanation = None;
        }
    }

    let computed_total_score: Decimal = questions.iter().map(|q| q.effective_score()).sum();

    Ok(Json(ExamDetail {
        exam,
        questions,
        computed_total_score,
    }))
}

/// PUT /api/courses/{course_id}/exams/{exam_id}
pub async fn update_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateQuiz>,
) -> AppResult<Quiz> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let existing = question_repo::find_exam_by_id(&pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if existing.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }

    let updated = question_repo::update_exam(&pool, exam_id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    Ok(Json(updated))
}

/// DELETE /api/courses/{course_id}/exams/{exam_id}
pub async fn delete_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let existing = question_repo::find_exam_by_id(&pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if existing.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }

    question_repo::delete_exam(&pool, exam_id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/courses/{course_id}/exams/{exam_id}/publish
///
/// 切换试卷的发布状态
pub async fn toggle_exam_publish(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((course_id, exam_id)): Path<(Uuid, Uuid)>,
) -> AppResult<serde_json::Value> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let existing = question_repo::find_exam_by_id(&pool, exam_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("试卷不存在"))?;

    if existing.course_id != course_id {
        return Err(not_found("试卷不属于该课程"));
    }

    let new_state = !existing.is_published;
    question_repo::set_exam_published(&pool, exam_id, new_state)
        .await
        .map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "exam_id": exam_id,
        "is_published": new_state,
        "message": if new_state { "试卷已发布" } else { "试卷已取消发布" }
    })))
}

// ---------------------------------------------------------------------------
// 试卷组题（Exam Composition）
// ---------------------------------------------------------------------------

/// GET /api/exams/{exam_id}/questions
pub async fn list_exam_questions(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(exam_id): Path<Uuid>,
) -> AppResult<Vec<QuestionInExam>> {
    ensure_exam_owner(&pool, &auth, exam_id).await?;
    let questions = question_repo::list_exam_questions(&pool, exam_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(questions))
}

/// POST /api/exams/{exam_id}/questions
///
/// 向试卷添加一道来自题库的题目（题目必须属于同一课程）
pub async fn add_question_to_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(exam_id): Path<Uuid>,
    Json(payload): Json<AddQuestionToExam>,
) -> Result<(StatusCode, Json<QuestionInExam>), (StatusCode, String)> {
    let exam = ensure_exam_owner(&pool, &auth, exam_id).await?;

    let question = question_repo::find_question_by_id(&pool, payload.question_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("题目不存在"))?;

    // 确保题目属于同一课程，防止跨课程组卷
    if question.course_id != exam.course_id {
        return Err(bad_request("该题目不属于试卷所在课程，无法加入"));
    }

    let entry = question_repo::add_question_to_exam(&pool, exam_id, &payload)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(entry)))
}

/// PUT /api/exams/{exam_id}/questions/{entry_id}
///
/// 更新某道题在试卷中的分值或排序
pub async fn update_question_in_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((exam_id, entry_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateQuestionInExam>,
) -> AppResult<QuestionInExam> {
    ensure_exam_owner(&pool, &auth, exam_id).await?;

    let updated = question_repo::update_question_in_exam(&pool, entry_id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("组题记录不存在"))?;

    Ok(Json(updated))
}

/// DELETE /api/exams/{exam_id}/questions/{entry_id}
///
/// 从试卷中移除一道题（不会删除题库中的原题）
pub async fn remove_question_from_exam(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path((exam_id, entry_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, (StatusCode, String)> {
    ensure_exam_owner(&pool, &auth, exam_id).await?;

    let rows = question_repo::remove_question_from_exam(&pool, entry_id)
        .await
        .map_err(internal_error)?;

    if rows == 0 {
        return Err(not_found("组题记录不存在"));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 题库导出 / 导入（XLSX 格式）
// ---------------------------------------------------------------------------

/// GET /api/courses/{course_id}/questions/export?chapter_id=&video_id=
///
/// 将课程题库导出为 Excel（.xlsx）文件。
/// 支持 chapter_id / video_id 过滤，chapter_id/video_id 随题目一并写入供参考，
/// 但导入时自动忽略。
pub async fn export_questions(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    Query(params): Query<ListQuestionsParams>,
) -> Result<Response, (StatusCode, String)> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let questions =
        question_repo::list_questions(&pool, course_id, params.chapter_id, params.video_id)
            .await
            .map_err(internal_error)?;

    let xlsx_bytes = build_questions_xlsx(&questions).map_err(|e| internal_error(e))?;

    let filename = format!(
        "questions_{}_{}.xlsx",
        &course_id.to_string()[..8],
        Utc::now().format("%Y%m%d"),
    );
    let content_disposition = format!("attachment; filename=\"{filename}\"");

    Ok((
        StatusCode::OK,
        [
            (
                header::CONTENT_TYPE,
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            ),
            (header::CONTENT_DISPOSITION, content_disposition.as_str()),
        ],
        xlsx_bytes,
    )
        .into_response())
}

/// POST /api/courses/{course_id}/questions/import
///
/// 通过 multipart/form-data 上传 Excel 文件（字段名 "file"）批量导入题目。
/// 可直接上传由 export 接口生成的文件，也可按照模板格式手动填写。
///
/// 注意：
/// - chapter_id / video_id 列在导入时始终忽略（置为 null），避免悬挂引用。
/// - 每道题独立校验，失败的题目跳过，其余正常写入。
/// - 通过校验的题目在同一事务中批量写入。
pub async fn import_questions(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(course_id): Path<Uuid>,
    mut multipart: Multipart,
) -> AppResult<ImportResult> {
    ensure_course_teacher(&pool, &auth, course_id).await?;

    let mut file_bytes: Option<Vec<u8>> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| bad_request(&e.to_string()))?
    {
        if field.name() == Some("file") {
            file_bytes = Some(
                field
                    .bytes()
                    .await
                    .map_err(|e| bad_request(&e.to_string()))?
                    .to_vec(),
            );
            break;
        }
    }

    let bytes = file_bytes
        .ok_or_else(|| bad_request("请通过 multipart/form-data 上传名为 'file' 的 Excel 文件"))?;

    let (valid, errors) =
        parse_questions_xlsx(&bytes).map_err(|e| bad_request(&e))?;

    let total = valid.len() + errors.len();
    let imported = valid.len();
    let failed = errors.len();

    if !valid.is_empty() {
        question_repo::import_questions_batch(&pool, course_id, auth.user_id, &valid)
            .await
            .map_err(internal_error)?;
    }

    Ok(Json(ImportResult {
        total,
        imported,
        failed,
        errors,
    }))
}

// ---------------------------------------------------------------------------
// XLSX 辅助函数
// ---------------------------------------------------------------------------

/// XLSX 列定义（顺序固定，导入解析依赖此顺序）
/// 0:题目类型 1:题目内容 2:选项A 3:选项B 4:选项C 5:选项D
/// 6:正确答案  7:解析   8:默认分值
const COL_HEADERS: &[&str] = &[
    "题目类型",
    "题目内容",
    "选项A",
    "选项B",
    "选项C",
    "选项D",
    "正确答案",
    "解析（可选）",
    "默认分值",
];
const COL_WIDTHS: &[f64] = &[18.0, 50.0, 22.0, 22.0, 22.0, 22.0, 18.0, 30.0, 12.0];

/// 根据 Question 列表生成 XLSX 字节流。
/// 生成两个 Sheet：
///   - "题目列表"：供导入使用的数据表（含表头）
///   - "填写说明"：字段说明，仅供参考
fn build_questions_xlsx(questions: &[Question]) -> Result<Vec<u8>, String> {
    use rust_xlsxwriter::{Color, Format, Workbook};

    let mut workbook = Workbook::new();

    // ── Sheet 1: 题目列表 ──────────────────────────────────────────────────
    {
        let ws = workbook.add_worksheet();
        ws.set_name("题目列表").map_err(|e| e.to_string())?;

        let hdr_fmt = Format::new()
            .set_bold()
            .set_background_color(Color::RGB(0x4472C4))
            .set_font_color(Color::White);

        for (col, (h, w)) in COL_HEADERS.iter().zip(COL_WIDTHS.iter()).enumerate() {
            let col = col as u16;
            ws.write_with_format(0, col, *h, &hdr_fmt)
                .map_err(|e| e.to_string())?;
            ws.set_column_width(col, *w)
                .map_err(|e| e.to_string())?;
        }

        for (row_idx, q) in questions.iter().enumerate() {
            let row = (row_idx + 1) as u32;

            // Col 0: question_type
            let qt_str = match q.question_type {
                QuestionType::SingleChoice => "single_choice",
                QuestionType::MultipleChoice => "multiple_choice",
                QuestionType::TrueFalse => "true_false",
                QuestionType::Essay => "essay",
            };
            ws.write(row, 0, qt_str).map_err(|e| e.to_string())?;

            // Col 1: content
            ws.write(row, 1, q.content.as_str())
                .map_err(|e| e.to_string())?;

            // Col 2-5: options A/B/C/D
            if let Some(opts) = &q.options {
                if let Some(arr) = opts.as_array() {
                    for (i, opt) in arr.iter().take(4).enumerate() {
                        let text = opt
                            .get("text")
                            .and_then(|t| t.as_str())
                            .unwrap_or("");
                        if !text.is_empty() {
                            ws.write(row, (2 + i) as u16, text)
                                .map_err(|e| e.to_string())?;
                        }
                    }
                }
            }

            // Col 6: correct_answer → 统一序列化为字符串
            let ans_str: String = match &q.correct_answer {
                Some(v) if v.is_string() => v.as_str().unwrap_or("").to_string(),
                Some(v) if v.is_array() => v
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                Some(v) if v.is_boolean() => {
                    if v.as_bool().unwrap() { "TRUE" } else { "FALSE" }.to_string()
                }
                _ => String::new(),
            };
            if !ans_str.is_empty() {
                ws.write(row, 6, ans_str.as_str())
                    .map_err(|e| e.to_string())?;
            }

            // Col 7: explanation
            if let Some(exp) = &q.explanation {
                if !exp.is_empty() {
                    ws.write(row, 7, exp.as_str())
                        .map_err(|e| e.to_string())?;
                }
            }

            // Col 8: default_score（Decimal → f64 via string）
            let score: f64 = q.default_score.to_string().parse().unwrap_or(5.0);
            ws.write(row, 8, score).map_err(|e| e.to_string())?;
        }
    }

    // ── Sheet 2: 填写说明 ──────────────────────────────────────────────────
    {
        let ws = workbook.add_worksheet();
        ws.set_name("填写说明").map_err(|e| e.to_string())?;
        ws.set_column_width(0, 18.0).map_err(|e| e.to_string())?;
        ws.set_column_width(1, 70.0).map_err(|e| e.to_string())?;

        let bold = Format::new().set_bold();
        let notes: &[(&str, &str)] = &[
            ("字段", "说明"),
            (
                "题目类型",
                "single_choice（单选）/ multiple_choice（多选）/ true_false（判断）/ essay（问答）",
            ),
            ("题目内容", "必填，不能为空"),
            ("选项A~D", "仅选择题填写；判断题/问答题可留空"),
            (
                "正确答案",
                "单选填 A/B/C/D；多选填 A,B,D；判断填 TRUE/FALSE；问答留空",
            ),
            ("解析", "可选"),
            ("默认分值", "数字，留空默认为 5"),
            (
                "导入注意",
                "chapter_id/video_id 导入后均置为空，可在系统内手动重新分配",
            ),
        ];

        for (row, (key, val)) in notes.iter().enumerate() {
            let row = row as u32;
            if row == 0 {
                ws.write_with_format(row, 0, *key, &bold)
                    .map_err(|e| e.to_string())?;
                ws.write_with_format(row, 1, *val, &bold)
                    .map_err(|e| e.to_string())?;
            } else {
                ws.write(row, 0, *key).map_err(|e| e.to_string())?;
                ws.write(row, 1, *val).map_err(|e| e.to_string())?;
            }
        }
    }

    workbook.save_to_buffer().map_err(|e| e.to_string())
}

/// 从 XLSX 字节流解析题目列表。
/// 优先读取名为"题目列表"的工作表，否则读第一个工作表。
/// 第一行为表头，从第二行开始解析数据。
/// 返回 (有效题目列表, 错误列表)；chapter_id / video_id 强制设为 None。
fn parse_questions_xlsx(bytes: &[u8]) -> Result<(Vec<ExportedQuestion>, Vec<ImportError>), String> {
    use calamine::{Data, Reader, Xlsx};
    use std::io::Cursor;

    // 辅助：取指定列的字符串值（Float/Int 转为字符串；空返回空串）
    let cell_str = |row: &[Data], col: usize| -> String {
        match row.get(col).unwrap_or(&Data::Empty) {
            Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => s.trim().to_string(),
            Data::Float(f) => f.to_string(),
            Data::Int(i) => i.to_string(),
            Data::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            _ => String::new(),
        }
    };

    // 辅助：取指定列的浮点值（空或无效返回 None）
    let cell_f64 = |row: &[Data], col: usize| -> Option<f64> {
        match row.get(col).unwrap_or(&Data::Empty) {
            Data::Float(f) => Some(*f),
            Data::Int(i) => Some(*i as f64),
            Data::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    };

    let cursor = Cursor::new(bytes.to_vec());
    let mut wb: Xlsx<_> =
        Xlsx::new(cursor).map_err(|e| format!("无法解析 Excel 文件: {e}"))?;

    let sheet_names = wb.sheet_names().to_owned();
    if sheet_names.is_empty() {
        return Err("Excel 文件中没有工作表".to_string());
    }
    // 优先使用"题目列表"，否则取第一个
    let target = sheet_names
        .iter()
        .find(|n| n.as_str() == "题目列表")
        .or(sheet_names.first())
        .cloned()
        .unwrap();

    let range = wb
        .worksheet_range(&target)
        .map_err(|e| format!("读取工作表「{target}」失败: {e}"))?;

    let mut valid: Vec<ExportedQuestion> = Vec::new();
    let mut errors: Vec<ImportError> = Vec::new();

    for (row_idx, row) in range.rows().enumerate().skip(1) {
        let line = row_idx + 1; // Excel 行号（表头为第1行，数据从第2行）
        let idx = row_idx;     // 从 1 开始的题目序号（跳过表头后第1题 = idx 1）

        // 完全空行直接跳过
        if row.iter().all(|c| matches!(c, Data::Empty)) {
            continue;
        }

        // ── 题目类型 ────────────────────────────────────────────────────────
        let qt_str = cell_str(row, 0);
        let question_type = match qt_str.to_lowercase().as_str() {
            "single_choice" => QuestionType::SingleChoice,
            "multiple_choice" => QuestionType::MultipleChoice,
            "true_false" => QuestionType::TrueFalse,
            "essay" => QuestionType::Essay,
            other => {
                errors.push(ImportError {
                    index: idx,
                    message: format!(
                        "第 {} 行：无效题目类型 \"{other}\"，应为 single_choice / multiple_choice / true_false / essay",
                        line + 1
                    ),
                });
                continue;
            }
        };

        // ── 题目内容 ────────────────────────────────────────────────────────
        let content = cell_str(row, 1);
        if content.is_empty() {
            errors.push(ImportError {
                index: idx,
                message: format!("第 {} 行：题目内容（B列）不能为空", line + 1),
            });
            continue;
        }

        // ── 选项 A/B/C/D ────────────────────────────────────────────────────
        let opt_keys = ["A", "B", "C", "D"];
        let options_arr: Vec<serde_json::Value> = opt_keys
            .iter()
            .enumerate()
            .filter_map(|(i, key)| {
                let text = cell_str(row, 2 + i);
                if text.is_empty() {
                    None
                } else {
                    Some(serde_json::json!({"key": key, "text": text}))
                }
            })
            .collect();

        let needs_options = matches!(
            question_type,
            QuestionType::SingleChoice | QuestionType::MultipleChoice
        );
        if needs_options && options_arr.is_empty() {
            errors.push(ImportError {
                index: idx,
                message: format!(
                    "第 {} 行：选择题必须在 C~F 列填写至少一个选项",
                    line + 1
                ),
            });
            continue;
        }
        let options = if options_arr.is_empty() {
            None
        } else {
            Some(serde_json::Value::Array(options_arr))
        };

        // ── 正确答案 ────────────────────────────────────────────────────────
        let ans_raw = cell_str(row, 6);
        let correct_answer: Option<serde_json::Value> = match question_type {
            QuestionType::SingleChoice if !ans_raw.is_empty() => {
                Some(serde_json::Value::String(ans_raw))
            }
            QuestionType::MultipleChoice if !ans_raw.is_empty() => {
                let parts: Vec<serde_json::Value> = ans_raw
                    .split(',')
                    .map(|s| serde_json::Value::String(s.trim().to_uppercase()))
                    .collect();
                Some(serde_json::Value::Array(parts))
            }
            QuestionType::TrueFalse if !ans_raw.is_empty() => {
                let b = matches!(
                    ans_raw.to_uppercase().as_str(),
                    "TRUE" | "1" | "是" | "对" | "正确"
                );
                Some(serde_json::Value::Bool(b))
            }
            _ => None,
        };

        // ── 解析 ─────────────────────────────────────────────────────────────
        let explanation = {
            let s = cell_str(row, 7);
            if s.is_empty() { None } else { Some(s) }
        };

        // ── 默认分值 ─────────────────────────────────────────────────────────
        let score_f = cell_f64(row, 8).unwrap_or(5.0);
        let default_score = score_f
            .to_string()
            .parse::<rust_decimal::Decimal>()
            .unwrap_or_else(|_| rust_decimal::Decimal::from(5));

        valid.push(ExportedQuestion {
            question_type,
            content,
            options,
            correct_answer,
            explanation,
            default_score,
            chapter_id: None,
            video_id: None,
        });
    }

    Ok((valid, errors))
}
