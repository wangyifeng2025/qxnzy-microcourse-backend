use sqlx::PgPool;
use uuid::Uuid;

use crate::models::enums::VideoStatus;
use crate::models::video::{CreateVideoRequest, UpdateVideoRequest, Video, VideoTranscode};

// ---------------------------------------------------------------------------
// Video CRUD
// ---------------------------------------------------------------------------

pub async fn create(
    pool: &PgPool,
    chapter_id: Uuid,
    payload: &CreateVideoRequest,
) -> Result<Video, sqlx::Error> {
    let sort_order = payload.sort_order.unwrap_or(0);
    sqlx::query_as!(
        Video,
        r#"
        INSERT INTO videos (chapter_id, title, description, sort_order)
        VALUES ($1, $2, $3, $4)
        RETURNING id, chapter_id, title, description, duration, original_url, cover_url,
                  status AS "status: _", sort_order, view_count, created_at, updated_at
        "#,
        chapter_id,
        payload.title,
        payload.description,
        sort_order,
    )
    .fetch_one(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Video>, sqlx::Error> {
    sqlx::query_as!(
        Video,
        r#"
        SELECT id, chapter_id, title, description, duration, original_url, cover_url,
               status AS "status: _", sort_order, view_count, created_at, updated_at
        FROM videos
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn find_by_chapter_id(
    pool: &PgPool,
    chapter_id: Uuid,
) -> Result<Vec<Video>, sqlx::Error> {
    sqlx::query_as!(
        Video,
        r#"
        SELECT id, chapter_id, title, description, duration, original_url, cover_url,
               status AS "status: _", sort_order, view_count, created_at, updated_at
        FROM videos
        WHERE chapter_id = $1
        ORDER BY sort_order ASC, created_at ASC
        "#,
        chapter_id
    )
    .fetch_all(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateVideoRequest,
) -> Result<Option<Video>, sqlx::Error> {
    sqlx::query_as!(
        Video,
        r#"
        UPDATE videos
        SET
            title       = COALESCE($2, title),
            description = COALESCE($3, description),
            sort_order  = COALESCE($4, sort_order),
            updated_at  = NOW()
        WHERE id = $1
        RETURNING id, chapter_id, title, description, duration, original_url, cover_url,
                  status AS "status: _", sort_order, view_count, created_at, updated_at
        "#,
        id,
        payload.title,
        payload.description,
        payload.sort_order,
    )
    .fetch_optional(pool)
    .await
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    status: VideoStatus,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"UPDATE videos SET status = $2, updated_at = NOW() WHERE id = $1"#,
        id,
        status as VideoStatus,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_original_url(
    pool: &PgPool,
    id: Uuid,
    original_url: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE videos SET original_url = $2, updated_at = NOW() WHERE id = $1",
        id,
        original_url
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_duration(pool: &PgPool, id: Uuid, duration: i32) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE videos SET duration = $2, updated_at = NOW() WHERE id = $1",
        id,
        duration
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_cover_url(pool: &PgPool, id: Uuid, cover_url: &str) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE videos SET cover_url = $2, updated_at = NOW() WHERE id = $1",
        id,
        cover_url
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM videos WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

// ---------------------------------------------------------------------------
// VideoTranscode 任务队列
// ---------------------------------------------------------------------------

/// 目标转码分辨率（高度像素）
pub const TRANSCODE_RESOLUTIONS: &[(&str, u32)] =
    &[("1080p", 1080), ("720p", 720), ("480p", 480), ("360p", 360)];

/// 为视频创建所有分辨率的转码任务（confirm-upload 时调用）
pub async fn create_transcode_tasks(
    pool: &PgPool,
    video_id: Uuid,
) -> Result<Vec<VideoTranscode>, sqlx::Error> {
    let mut tasks = Vec::with_capacity(TRANSCODE_RESOLUTIONS.len());
    for &(resolution, _) in TRANSCODE_RESOLUTIONS {
        // ON CONFLICT DO NOTHING 保证接口幂等
        let task = sqlx::query_as!(
            VideoTranscode,
            r#"
            INSERT INTO video_transcodes (video_id, resolution)
            VALUES ($1, $2)
            ON CONFLICT (video_id, resolution) DO UPDATE
                SET status = EXCLUDED.status
            RETURNING id, video_id, resolution, playlist_url, file_size,
                      status AS "status: _", created_at, updated_at
            "#,
            video_id,
            resolution,
        )
        .fetch_one(pool)
        .await?;
        tasks.push(task);
    }
    Ok(tasks)
}

pub async fn find_transcodes_by_video_id(
    pool: &PgPool,
    video_id: Uuid,
) -> Result<Vec<VideoTranscode>, sqlx::Error> {
    sqlx::query_as!(
        VideoTranscode,
        r#"
        SELECT id, video_id, resolution, playlist_url, file_size,
               status AS "status: _", created_at, updated_at
        FROM video_transcodes
        WHERE video_id = $1
        ORDER BY created_at ASC
        "#,
        video_id
    )
    .fetch_all(pool)
    .await
}

/// 原子性认领同一视频的所有 pending 转码任务（一次 FFmpeg 多路输出，避免重复解码）
///
/// 用 CTE 锁定最早一条 pending 任务所在视频，再批量认领该视频的全部 pending 任务。
/// `FOR UPDATE SKIP LOCKED` 保证多 Worker 并发时不重复认领。
pub async fn claim_pending_transcode_batch(
    pool: &PgPool,
) -> Result<Vec<VideoTranscode>, sqlx::Error> {
    sqlx::query_as::<_, VideoTranscode>(
        r#"
        WITH locked AS (
            SELECT video_id
            FROM video_transcodes
            WHERE status = 'pending'::transcode_status
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        UPDATE video_transcodes t
        SET status = 'processing'::transcode_status, updated_at = NOW()
        FROM locked
        WHERE t.video_id = locked.video_id
          AND t.status = 'pending'::transcode_status
        RETURNING t.id, t.video_id, t.resolution, t.playlist_url, t.file_size,
                  t.status, t.created_at, t.updated_at
        "#,
    )
    .fetch_all(pool)
    .await
}

/// 原子性认领一个 pending 转码任务（FOR UPDATE SKIP LOCKED，支持多 Worker 并发）
pub async fn claim_pending_transcode(pool: &PgPool) -> Result<Option<VideoTranscode>, sqlx::Error> {
    sqlx::query_as!(
        VideoTranscode,
        r#"
        UPDATE video_transcodes
        SET status = 'processing'::transcode_status, updated_at = NOW()
        WHERE id = (
            SELECT id
            FROM video_transcodes
            WHERE status = 'pending'::transcode_status
            ORDER BY created_at ASC
            LIMIT 1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, video_id, resolution, playlist_url, file_size,
                  status AS "status: _", created_at, updated_at
        "#,
    )
    .fetch_optional(pool)
    .await
}

/// 标记转码任务完成并记录 HLS 播放列表 key 及文件总大小
pub async fn complete_transcode(
    pool: &PgPool,
    id: Uuid,
    playlist_key: &str,
    file_size: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE video_transcodes
        SET status = 'done'::transcode_status,
            playlist_url = $2,
            file_size    = $3,
            updated_at   = NOW()
        WHERE id = $1
        "#,
        id,
        playlist_key,
        file_size,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// 标记转码任务失败
pub async fn fail_transcode(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE video_transcodes
        SET status = 'failed'::transcode_status, updated_at = NOW()
        WHERE id = $1
        "#,
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// 检查视频的所有转码任务是否全部成功完成（没有 pending/processing/failed 的任务）
pub async fn all_transcodes_done(pool: &PgPool, video_id: Uuid) -> Result<bool, sqlx::Error> {
    let not_done_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM video_transcodes
        WHERE video_id = $1 AND status <> 'done'::transcode_status
        "#,
        video_id
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(1);
    Ok(not_done_count == 0)
}

/// 检查视频是否有任何失败的转码任务
pub async fn has_failed_transcode(pool: &PgPool, video_id: Uuid) -> Result<bool, sqlx::Error> {
    let failed_count: i64 = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*) FROM video_transcodes
        WHERE video_id = $1 AND status = 'failed'::transcode_status
        "#,
        video_id
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);
    Ok(failed_count > 0)
}
