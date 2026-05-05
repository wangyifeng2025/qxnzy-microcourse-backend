-- 独立社区话题（不依附任何课程，由教师/管理员开设，任意用户可加入）

-- 话题主表
CREATE TABLE IF NOT EXISTS community_topics (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id    UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title        VARCHAR(200) NOT NULL,
    content      TEXT        NOT NULL,
    is_pinned    BOOLEAN     NOT NULL DEFAULT FALSE,
    reply_count  INTEGER     NOT NULL DEFAULT 0,
    member_count INTEGER     NOT NULL DEFAULT 1,  -- 创建者自动计入
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 话题成员表（JOIN / LEAVE）
CREATE TABLE IF NOT EXISTS community_topic_members (
    topic_id  UUID        NOT NULL REFERENCES community_topics (id) ON DELETE CASCADE,
    user_id   UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (topic_id, user_id)
);

-- 话题回复表
CREATE TABLE IF NOT EXISTS community_topic_replies (
    id                UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    topic_id          UUID        NOT NULL REFERENCES community_topics (id) ON DELETE CASCADE,
    author_id         UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    content           TEXT        NOT NULL,
    reply_to_reply_id UUID        REFERENCES community_topic_replies (id) ON DELETE SET NULL,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- @提及记录（topic_id 和 reply_id 二选一，不可同时为空或同时非空）
CREATE TABLE IF NOT EXISTS community_topic_mentions (
    id                 UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    topic_id           UUID        REFERENCES community_topics (id) ON DELETE CASCADE,
    reply_id           UUID        REFERENCES community_topic_replies (id) ON DELETE CASCADE,
    mentioned_user_id  UUID        NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT community_mention_must_have_source CHECK (
        (topic_id IS NOT NULL)::int + (reply_id IS NOT NULL)::int = 1
    )
);

-- 话题列表：置顶优先，然后按发布时间倒序
CREATE INDEX IF NOT EXISTS idx_community_topics_list
    ON community_topics (is_pinned DESC, created_at DESC, id DESC);

-- 通过用户查其加入的话题
CREATE INDEX IF NOT EXISTS idx_community_topic_members_user
    ON community_topic_members (user_id);

-- 回复列表：时间正序（楼层顺序）
CREATE INDEX IF NOT EXISTS idx_community_topic_replies_topic
    ON community_topic_replies (topic_id, created_at ASC, id ASC);

-- 被@的用户查询
CREATE INDEX IF NOT EXISTS idx_community_topic_mentions_mentioned
    ON community_topic_mentions (mentioned_user_id);
