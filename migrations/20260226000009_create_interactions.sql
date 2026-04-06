-- 视频内互动问答点：播放到指定秒数时弹出
CREATE TABLE video_questions (
    id              UUID          PRIMARY KEY DEFAULT gen_random_uuid(),
    video_id        UUID          NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    position_seconds INT          NOT NULL,                         -- 触发秒数
    question_type   question_type  NOT NULL,
    content         TEXT,
    options         JSONB,
    correct_answer  JSONB,
    explanation     TEXT,
    created_at      TIMESTAMPTZ   NOT NULL DEFAULT NOW()
);

-- 学生对视频问答点的回答记录
CREATE TABLE video_question_responses (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    question_id   UUID        NOT NULL REFERENCES video_questions(id) ON DELETE CASCADE,
    answer       JSONB,
    is_correct   BOOLEAN,
    responded_at TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);
