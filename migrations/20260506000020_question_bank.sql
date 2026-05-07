-- 题库（独立题目，可被多个试卷复用）
-- 以小节（video）为最细粒度挂载；chapter_id 可派生，但冗余存储方便按章节筛选
CREATE TABLE IF NOT EXISTS questions (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    course_id      UUID        NOT NULL REFERENCES courses(id)  ON DELETE CASCADE,
    chapter_id     UUID                 REFERENCES chapters(id) ON DELETE SET NULL,
    video_id       UUID                 REFERENCES videos(id)   ON DELETE SET NULL,
    created_by     UUID        NOT NULL REFERENCES users(id),
    question_type  question_type NOT NULL,
    content        TEXT        NOT NULL,
    -- 选项（选择题）：[{"key":"A","text":"..."},{"key":"B","text":"..."}]
    options        JSONB,
    -- 正确答案（选择题 / 判断题自动评分用）
    -- 单选："A"；多选：["A","C"]；判断：true/false；问答：null（人工评分）
    correct_answer JSONB,
    explanation    TEXT,
    default_score  NUMERIC(6, 2) NOT NULL DEFAULT 5,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_questions_course_id   ON questions(course_id);
CREATE INDEX IF NOT EXISTS idx_questions_chapter_id  ON questions(chapter_id);
CREATE INDEX IF NOT EXISTS idx_questions_video_id    ON questions(video_id);
CREATE INDEX IF NOT EXISTS idx_questions_created_by  ON questions(created_by);

-- 试卷-题目关联表（从题库组卷，多对多）
-- quizzes 表已存在（作为"试卷"使用），此表是其与题库的 JOIN 表
CREATE TABLE IF NOT EXISTS exam_questions (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    quiz_id     UUID        NOT NULL REFERENCES quizzes(id)   ON DELETE CASCADE,
    question_id UUID        NOT NULL REFERENCES questions(id) ON DELETE CASCADE,
    -- 该题在本试卷中的分值；NULL 表示沿用 questions.default_score
    score       NUMERIC(6, 2),
    sort_order  INT         NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(quiz_id, question_id)
);

CREATE INDEX IF NOT EXISTS idx_exam_questions_quiz_id     ON exam_questions(quiz_id);
CREATE INDEX IF NOT EXISTS idx_exam_questions_question_id ON exam_questions(question_id);
