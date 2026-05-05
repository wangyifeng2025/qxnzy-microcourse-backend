-- 课程话题表
CREATE TABLE IF NOT EXISTS course_topics (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    course_id UUID NOT NULL REFERENCES courses (id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title VARCHAR(200) NOT NULL,
    content TEXT NOT NULL,
    is_pinned BOOLEAN NOT NULL DEFAULT FALSE,
    reply_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 话题回复表
CREATE TABLE IF NOT EXISTS course_topic_replies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    topic_id UUID NOT NULL REFERENCES course_topics (id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    -- 可选：回复某一条具体的回复（楼中楼），删除时置空
    reply_to_reply_id UUID REFERENCES course_topic_replies (id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- @提及记录（topic_id 和 reply_id 二选一，不可同时为空或同时非空）
CREATE TABLE IF NOT EXISTS course_topic_mentions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid (),
    topic_id UUID REFERENCES course_topics (id) ON DELETE CASCADE,
    reply_id UUID REFERENCES course_topic_replies (id) ON DELETE CASCADE,
    mentioned_user_id UUID NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT mention_must_have_source CHECK (
        (topic_id IS NOT NULL)::int + (reply_id IS NOT NULL)::int = 1
    )
);

-- 索引：按课程查话题（置顶优先，然后按时间倒序）
CREATE INDEX IF NOT EXISTS idx_course_topics_course ON course_topics (
    course_id,
    is_pinned DESC,
    created_at DESC,
    id DESC
);
-- 索引：按话题查回复（时间正序）
CREATE INDEX IF NOT EXISTS idx_course_topic_replies_topic ON course_topic_replies (
    topic_id,
    created_at ASC,
    id ASC
);
-- 索引：查某用户被@的记录
CREATE INDEX IF NOT EXISTS idx_course_topic_mentions_mentioned ON course_topic_mentions (mentioned_user_id);