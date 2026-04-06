CREATE TABLE videos (
    id           UUID         PRIMARY KEY DEFAULT gen_random_uuid(),
    chapter_id   UUID         NOT NULL REFERENCES chapters(id) ON DELETE CASCADE,
    title        VARCHAR(200) NOT NULL,
    description  TEXT,
    duration     INT          NOT NULL DEFAULT 0,  -- 视频时长（秒）
    original_url TEXT,                             -- MinIO 原始文件路径（上传后写入）
    cover_url    TEXT,                             -- FFmpeg 截图封面 URL
    status       video_status NOT NULL DEFAULT 'pending',
    sort_order   INT          NOT NULL DEFAULT 0,
    view_count   INT          NOT NULL DEFAULT 0,
    created_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ  NOT NULL DEFAULT NOW()
);

-- 视频多清晰度转码记录，每条记录对应一个 HLS 流
CREATE TABLE video_transcodes (
    id           UUID             PRIMARY KEY DEFAULT gen_random_uuid(),
    video_id     UUID             NOT NULL REFERENCES videos(id) ON DELETE CASCADE,
    resolution   VARCHAR(10)      NOT NULL,        -- '1080p' / '720p' / '480p' / '360p'
    playlist_url TEXT,                             -- HLS m3u8 播放列表 URL
    file_size    BIGINT,                           -- 字节数
    status       transcode_status NOT NULL DEFAULT 'pending',
    created_at   TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    updated_at   TIMESTAMPTZ      NOT NULL DEFAULT NOW(),
    UNIQUE (video_id, resolution)
);
