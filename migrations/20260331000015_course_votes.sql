-- 新增 course_votes 表
CREATE TABLE course_votes (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    course_id  UUID        NOT NULL REFERENCES courses(id) ON DELETE CASCADE,
    voted_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, course_id)
);

CREATE INDEX idx_course_votes_course_id ON course_votes(course_id);

-- courses 表增加 vote_count 列
ALTER TABLE courses
    ADD COLUMN vote_count BIGINT NOT NULL DEFAULT 0
        CONSTRAINT vote_count_non_negative CHECK (vote_count >= 0);
