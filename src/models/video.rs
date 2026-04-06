use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::enums::{TranscodeStatus, VideoStatus};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Video {
    pub id: Uuid,
    pub chapter_id: Uuid,
    pub title: String,
    pub description: Option<String>,
    /// 视频时长（秒），上传完成后由客户端提供或 FFprobe 探测
    pub duration: i32,
    /// MinIO 对象 key（格式：raw/{video_id}/{filename}）
    pub original_url: Option<String>,
    /// FFmpeg 截帧封面的 MinIO 对象 key
    pub cover_url: Option<String>,
    pub status: VideoStatus,
    pub sort_order: i32,
    pub view_count: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// POST /api/chapters/:chapter_id/videos 请求体
#[derive(Debug, Deserialize)]
pub struct CreateVideoRequest {
    pub title: String,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

/// PUT /api/videos/:id 请求体
#[derive(Debug, Deserialize)]
pub struct UpdateVideoRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i32>,
}

// ---------------------------------------------------------------------------
// 上传流程专用类型

/// POST /api/videos/:id/upload-url 请求体（申请前端直传预签名 URL）
#[derive(Debug, Deserialize)]
pub struct RequestUploadUrlRequest {
    /// 原始文件名（用于构建 MinIO 对象 key）
    pub filename: String,
}

/// 申请上传 URL 的响应
#[derive(Debug, Serialize)]
pub struct RequestUploadUrlResponse {
    /// MinIO 预签名 PUT URL，前端用此 URL 直接上传视频文件
    pub upload_url: String,
    /// MinIO 对象 key，确认上传时需要携带
    pub object_key: String,
    /// 有效期（秒）
    pub expires_in: u64,
}

/// POST /api/videos/:id/confirm-upload 请求体（上传完成后通知后端）
#[derive(Debug, Deserialize)]
pub struct ConfirmUploadRequest {
    /// MinIO 对象 key（与申请 URL 时返回的一致）
    pub object_key: String,
    /// 视频时长（秒），可选；服务端会优先用 `ffprobe` 对对象探测真实时长。
    /// 若仅在 `loadedmetadata` 之前上报，浏览器常误报 `1` 或无效值，应以服务端探测为准。
    pub duration: Option<i32>,
}

/// 视频详情（含转码任务列表）
#[derive(Debug, Serialize)]
pub struct VideoDetail {
    #[serde(flatten)]
    pub video: Video,
    pub transcodes: Vec<VideoTranscode>,
}

// ---------------------------------------------------------------------------
// HLS 播放 URL 签发相关类型
// ---------------------------------------------------------------------------

/// POST /api/videos/:id/hls-url 请求体
#[derive(Debug, Deserialize)]
pub struct CreateHlsUrlRequest {
    /// 期望的清晰度（默认 720p）
    pub resolution: Option<String>,
    /// Token 有效期（秒），默认 600 秒（10 分钟）
    pub ttl_seconds: Option<u64>,
}

/// HLS 播放 URL 响应
#[derive(Debug, Serialize)]
pub struct CreateHlsUrlResponse {
    /// 可直接用于 hls.js / Safari 播放的完整 m3u8 URL（已带 hls_token）
    pub playlist_url: String,
    /// 过期时间戳（秒，UNIX 时间）
    pub expires_at: usize,
}

// ---------------------------------------------------------------------------

/// 视频多清晰度转码记录，每条对应一个 HLS 流
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct VideoTranscode {
    pub id: Uuid,
    pub video_id: Uuid,
    /// '1080p' / '720p' / '480p' / '360p'
    pub resolution: String,
    /// HLS m3u8 播放列表在 MinIO 中的对象 key
    pub playlist_url: Option<String>,
    /// 该分辨率所有分片文件的总字节数
    pub file_size: Option<i64>,
    pub status: TranscodeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
