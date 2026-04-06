-- 学习进度：每个用户对每个视频的观看进度
CREATE TABLE learning_progress (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id         UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    video_id        UUID        NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    last_position   INT         NOT NULL DEFAULT 0,   -- 上次播放位置（秒）
    watched_duration INT       NOT NULL DEFAULT 0,   -- 累计观看时长（秒）
    progress_pct    NUMERIC(5,2) NOT NULL DEFAULT 0, -- 0-100
    is_completed    BOOLEAN     NOT NULL DEFAULT FALSE,
    completed_at    TIMESTAMPTZ,
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (user_id, video_id)
);
