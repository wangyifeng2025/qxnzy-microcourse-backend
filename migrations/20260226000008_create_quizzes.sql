-- 测试/作业：可挂到课程或章节
CREATE TABLE quizzes (
    id           UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    course_id    UUID          NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    chapter_id   UUID          REFERENCES chapters(id) ON DELETE CASCADE,  -- NULL 表示课程级测试
    title        VARCHAR(200) NOT NULL,
    description  TEXT,
    time_limit   INT,                                              -- 限时（分钟），NULL 不限时
    total_score  NUMERIC(6,2)  NOT NULL DEFAULT 100,
    pass_score   NUMERIC(6,2)  NOT NULL DEFAULT 60,
    max_attempts INT,                                              -- 最大尝试次数，NULL 不限次
    is_published BOOLEAN       NOT NULL DEFAULT FALSE,
    created_at   TIMESTAMPTZ   NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

-- 测试题目
CREATE TABLE quiz_questions (
    id             UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    quiz_id        UUID          NOT NULL REFERENCES quizzes(id) ON DELETE CASCADE,
    question_type  question_type NOT NULL,
    content        TEXT         NOT NULL,
    options        JSONB,                                           -- 选项数组
    correct_answer JSONB,                                           -- 正确答案
    score          NUMERIC(6,2),
    explanation    TEXT,                                            -- 解析
    sort_order     INT          NOT NULL DEFAULT 0,
    created_at     TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- 答题记录
CREATE TABLE quiz_attempts (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    quiz_id     UUID        NOT NULL REFERENCES quizzes(id) ON DELETE CASCADE,
    score       NUMERIC(6,2),                                       -- NULL 表示未评分
    answers     JSONB,                                              -- 用户提交的答案
    is_graded   BOOLEAN     NOT NULL DEFAULT FALSE,
    started_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    submitted_at TIMESTAMPTZ,
    time_spent   INT                                                -- 答题耗时（秒）
);
