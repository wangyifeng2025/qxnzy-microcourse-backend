use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Extension, Path, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
    Json,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::middleware::auth::AuthContext;
use crate::models::enums::{UserRole, VideoStatus};
use crate::models::video::{
    ConfirmUploadRequest, CreateHlsUrlRequest, CreateHlsUrlResponse, CreateVideoRequest,
    RequestUploadUrlRequest, RequestUploadUrlResponse, UpdateVideoRequest, Video, VideoDetail,
};
use crate::repositories::{chapter as chapter_repo, course as course_repo, video as video_repo};
use crate::storage::AppStorage;
use crate::utils::filename::sanitize_filename;
use crate::utils::ffprobe;
use crate::utils::jwt::{decode_hls_token, encode_hls_token};

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    tracing::error!("{}", e);
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

fn bad_request(msg: &str) -> (StatusCode, String) {
    (StatusCode::BAD_REQUEST, msg.to_string())
}

fn not_found(msg: &str) -> (StatusCode, String) {
    (StatusCode::NOT_FOUND, msg.to_string())
}

fn forbidden(msg: &str) -> (StatusCode, String) {
    (StatusCode::FORBIDDEN, msg.to_string())
}

/// 检查当前用户是否有权操作该章节所属课程（教师本人或管理员）
async fn ensure_chapter_owner(
    pool: &PgPool,
    auth: &AuthContext,
    chapter_id: Uuid,
) -> Result<(), (StatusCode, String)> {
    if auth.role == UserRole::Admin {
        return Ok(());
    }
    let chapter = chapter_repo::find_by_id(pool, chapter_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("章节不存在"))?;

    let course = course_repo::find_by_id(pool, chapter.course_id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("课程不存在"))?;

    if course.teacher_id != auth.user_id {
        return Err(forbidden("仅课程教师或管理员可操作视频"));
    }
    Ok(())
}

/// 检查当前用户是否有权操作该视频
async fn ensure_video_owner(
    pool: &PgPool,
    auth: &AuthContext,
    video: &Video,
) -> Result<(), (StatusCode, String)> {
    ensure_chapter_owner(pool, auth, video.chapter_id).await
}

// ---------------------------------------------------------------------------
// 视频基础 CRUD
// ---------------------------------------------------------------------------

/// GET /api/chapters/:chapter_id/videos
pub async fn list_videos(
    State(pool): State<PgPool>,
    Path(chapter_id): Path<Uuid>,
) -> AppResult<Vec<Video>> {
    let videos: Vec<Video> = video_repo::find_by_chapter_id(&pool, chapter_id)
        .await
        .map_err(internal_error)?;
    Ok(Json(videos))
}

/// GET /api/videos/:id
pub async fn get_video(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> AppResult<VideoDetail> {
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    let transcodes = video_repo::find_transcodes_by_video_id(&pool, id)
        .await
        .map_err(internal_error)?;

    Ok(Json(VideoDetail { video, transcodes }))
}

/// POST /api/chapters/:chapter_id/videos（仅 Teacher/Admin，且为课程创建者）
pub async fn create_video(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(chapter_id): Path<Uuid>,
    Json(payload): Json<CreateVideoRequest>,
) -> Result<(StatusCode, Json<Video>), (StatusCode, String)> {
    if payload.title.trim().is_empty() {
        return Err(bad_request("视频标题不能为空"));
    }

    ensure_chapter_owner(&pool, &auth, chapter_id).await?;

    let video = video_repo::create(&pool, chapter_id, &payload)
        .await
        .map_err(internal_error)?;

    Ok((StatusCode::CREATED, Json(video)))
}

/// PUT /api/videos/:id
pub async fn update_video(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<UpdateVideoRequest>,
) -> AppResult<Video> {
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    ensure_video_owner(&pool, &auth, &video).await?;

    let updated = video_repo::update(&pool, id, &payload)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    Ok(Json(updated))
}

/// DELETE /api/videos/:id
pub async fn delete_video(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, (StatusCode, String)> {
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    ensure_video_owner(&pool, &auth, &video).await?;

    // 先删除 MinIO：原始文件、各清晰度 HLS、封面；再删库（video_transcodes 由 ON DELETE CASCADE 清理）
    storage
        .delete_video_assets(id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    video_repo::delete(&pool, id)
        .await
        .map_err(internal_error)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// 上传流程：申请预签名 URL → 前端直传 → 确认上传
// ---------------------------------------------------------------------------

/// POST /api/videos/:id/upload-url
///
/// 为已创建的视频记录申请一个 MinIO 预签名 PUT URL。
/// 前端拿到此 URL 后，直接用 HTTP PUT 将视频文件上传到 MinIO，
/// 无需经过后端服务器，大幅降低带宽压力。
///
/// 预签名 URL 有效期 1 小时。
pub async fn request_upload_url(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<RequestUploadUrlRequest>,
) -> AppResult<RequestUploadUrlResponse> {
    if payload.filename.trim().is_empty() {
        return Err(bad_request("filename 不能为空"));
    }

    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    ensure_video_owner(&pool, &auth, &video).await?;

    // 只允许在 pending 状态申请上传 URL
    if video.status != VideoStatus::Pending {
        return Err(bad_request("该视频已上传或正在转码，无法重复申请上传 URL"));
    }

    // 对象 key：raw/{video_id}/{原始文件名}，保留文件名方便调试
    let object_key = format!("raw/{}/{}", id, sanitize_filename(&payload.filename));

    // 预签名有效期 1 小时
    const EXPIRES_SECS: u64 = 3600;
    let upload_url = storage
        .presigned_put_url(&object_key, EXPIRES_SECS)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    // 提前将 object_key 写入数据库，方便 confirm-upload 时校验
    video_repo::update_original_url(&pool, id, &object_key)
        .await
        .map_err(internal_error)?;

    Ok(Json(RequestUploadUrlResponse {
        upload_url,
        object_key,
        expires_in: EXPIRES_SECS,
    }))
}

/// POST /api/videos/:id/confirm-upload
///
/// 前端完成直传后调用此接口。后端将：
/// 1. 验证对象是否真实存在于 MinIO
/// 2. 将视频状态更新为 processing
/// 3. 在 video_transcodes 表中插入 4 个分辨率的转码任务（pending）
/// 4. 后台 Worker 将异步消费这些任务
pub async fn confirm_upload(
    State(pool): State<PgPool>,
    Extension(auth): Extension<AuthContext>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path(id): Path<Uuid>,
    Json(payload): Json<ConfirmUploadRequest>,
) -> AppResult<serde_json::Value> {
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    ensure_video_owner(&pool, &auth, &video).await?;

    if video.status != VideoStatus::Pending {
        return Err(bad_request("该视频状态不允许确认上传"));
    }

    // 校验 object_key 与数据库记录一致
    let stored_key = video
        .original_url
        .as_deref()
        .ok_or_else(|| bad_request("请先申请上传 URL"))?;

    if stored_key != payload.object_key {
        return Err(bad_request("object_key 与记录不一致"));
    }

    // 验证文件是否真实存在于对象存储
    if !storage.object_exists(&payload.object_key).await {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "对象存储中尚未找到该文件，请确认上传已完成".to_string(),
        ));
    }

    // 视频时长：优先用 ffprobe 对 MinIO 对象探测（避免前端 metadata 未就绪时误报 1 秒等问题）
    const PRESIGN_GET_TTL_SECS: u64 = 600;
    let probed = match storage
        .presigned_get_url(&payload.object_key, PRESIGN_GET_TTL_SECS)
        .await
    {
        Ok(url) => ffprobe::probe_duration_seconds(&url).await,
        Err(e) => {
            tracing::warn!(error = %e, video_id = %id, "无法生成预签名 GET URL，跳过 ffprobe 时长探测");
            Err(e)
        }
    };

    match probed {
        Ok(secs) => {
            video_repo::update_duration(&pool, id, secs)
                .await
                .map_err(internal_error)?;
            tracing::info!(video_id = %id, duration_secs = secs, "ffprobe 探测视频时长成功");
        }
        Err(e) => {
            tracing::warn!(error = %e, video_id = %id, "ffprobe 探测失败，回退使用客户端上报时长（若有）");
            if let Some(duration) = payload.duration {
                if duration > 0 {
                    video_repo::update_duration(&pool, id, duration)
                        .await
                        .map_err(internal_error)?;
                }
            }
        }
    }

    // 将视频状态更新为 processing，并创建转码任务
    video_repo::update_status(&pool, id, VideoStatus::Processing)
        .await
        .map_err(internal_error)?;

    let tasks = video_repo::create_transcode_tasks(&pool, id)
        .await
        .map_err(internal_error)?;

    tracing::info!(
        video_id = %id,
        task_count = tasks.len(),
        "视频上传已确认，转码任务已入队"
    );

    Ok(Json(serde_json::json!({
        "message": "上传已确认，转码任务已创建",
        "task_count": tasks.len(),
    })))
}

/// GET /api/videos/:id/transcodes
pub async fn get_transcodes(
    State(pool): State<PgPool>,
    Path(id): Path<Uuid>,
) -> AppResult<serde_json::Value> {
    // 确保视频存在
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    let transcodes = video_repo::find_transcodes_by_video_id(&pool, id)
        .await
        .map_err(internal_error)?;

    Ok(Json(serde_json::json!({
        "video_id": id,
        "video_status": video.status,
        "transcodes": transcodes,
    })))
}

// ---------------------------------------------------------------------------
// HLS 播放 URL 签发（动态 URL 签名）
// ---------------------------------------------------------------------------

/// POST /api/videos/:id/hls-url
///
/// 使用登录 Token 换取一个短效的 HLS 播放 URL，URL 中携带 hls_token（JWT），
/// 可直接在任何支持 HLS 的播放器中使用（含 Safari / 移动端原生播放器）。
pub async fn create_hls_url(
    State(pool): State<PgPool>,
    Extension(_auth): Extension<AuthContext>,
    Path(id): Path<Uuid>,
    Json(payload): Json<CreateHlsUrlRequest>,
) -> AppResult<CreateHlsUrlResponse> {
    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    if video.status != VideoStatus::Ready {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "视频尚未转码完成，暂无法播放".to_string(),
        ));
    }

    // 默认清晰度 720p
    let resolution = payload
        .resolution
        .as_deref()
        .unwrap_or("720p")
        .to_string();

    // 默认 10 分钟有效期
    let ttl_secs: usize = payload.ttl_seconds.unwrap_or(600) as usize;

    let (token, exp) = encode_hls_token(id, &resolution, ttl_secs)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let playlist_url = format!(
        "/api/videos/{}/hls/{}/playlist.m3u8?hls_token={}",
        id, resolution, token
    );

    Ok(Json(CreateHlsUrlResponse {
        playlist_url,
        expires_at: exp,
    }))
}

// ---------------------------------------------------------------------------
// HLS 后端代理流式传输
// ---------------------------------------------------------------------------

/// GET /api/videos/:id/hls/:resolution/playlist.m3u8?hls_token=...
///
/// 从 MinIO 私有存储下载 HLS 播放列表，将其中的分片相对路径
/// 改写为指向后端自身的代理路径后返回给前端播放器。
///
/// 改写规则：
///   segment_0001.ts  →  /api/videos/:id/hls/:resolution/segment_0001.ts
///
/// 这样播放器请求每个分片时会经由后端鉴权后再从 MinIO 取数据，
/// 完全绕过 MinIO 的访问控制，bucket 无需设为 public。
pub async fn hls_playlist(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path((id, resolution)): Path<(Uuid, String)>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // 从 query 中提取并验证 hls_token
    let token = q
        .get("hls_token")
        .ok_or_else(|| bad_request("缺少 hls_token"))?;

    let claims = decode_hls_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;

    if claims.vid != id || claims.res != resolution {
        return Err((
            StatusCode::FORBIDDEN,
            "播放链接与视频不匹配".to_string(),
        ));
    }

    let video = video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    if video.status != VideoStatus::Ready {
        return Err((
            StatusCode::UNPROCESSABLE_ENTITY,
            "视频尚未转码完成".to_string(),
        ));
    }

    let playlist_key = format!("hls/{id}/{resolution}/playlist.m3u8");

    let raw_bytes = storage
        .download_bytes(&playlist_key)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("读取播放列表失败：{e}")))?;

    let raw_content = String::from_utf8(raw_bytes)
        .map_err(|_| (StatusCode::INTERNAL_SERVER_ERROR, "m3u8 编码异常".to_string()))?;

    // 将分片相对路径改写为后端代理绝对路径，并携带同一个 hls_token
    let rewritten: String = raw_content
        .lines()
        .map(|line| {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') && trimmed.ends_with(".ts") {
                format!("/api/videos/{id}/hls/{resolution}/{trimmed}?hls_token={token}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    tracing::debug!(
        video_id = %id,
        resolution = %resolution,
        "HLS 播放列表代理成功"
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/x-mpegURL")
        // 不缓存 m3u8：播放器会频繁重新请求以探测直播状态，VOD 虽然不变，
        // 但 no-cache 更安全，避免路径改写后被老缓存命中
        .header(header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .body(Body::from(rewritten))
        .map_err(internal_error)
}

/// GET /api/videos/:id/hls/:resolution/:segment?hls_token=...
///
/// 从 MinIO 私有存储拉取单个 TS 分片并流式透传给客户端。
///
/// TS 分片内容一旦生成就不再变化，设置长期强缓存（1年）以减少重复传输。
/// 路由注册时需放在 `playlist.m3u8` 路由之后，让静态字面量优先匹配。
pub async fn hls_segment(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Path((id, resolution, segment)): Path<(Uuid, String, String)>,
    Query(q): Query<std::collections::HashMap<String, String>>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // 简单校验 segment 文件名，防止路径穿越
    if segment.contains('/') || segment.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "非法的分片名称".to_string()));
    }

    // 从 query 中提取并验证 hls_token
    let token = q
        .get("hls_token")
        .ok_or_else(|| bad_request("缺少 hls_token"))?;

    let claims = decode_hls_token(token)
        .map_err(|e| (StatusCode::UNAUTHORIZED, e))?;

    if claims.vid != id || claims.res != resolution {
        return Err((
            StatusCode::FORBIDDEN,
            "播放链接与视频不匹配".to_string(),
        ));
    }

    // 验证视频存在（不再检查 status，处理中也允许播放已完成的分片）
    video_repo::find_by_id(&pool, id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("视频不存在"))?;

    let segment_key = format!("hls/{id}/{resolution}/{segment}");

    let bytes = storage
        .download_bytes(&segment_key)
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("读取视频分片失败：{e}")))?;

    tracing::debug!(
        video_id = %id,
        resolution = %resolution,
        segment = %segment,
        bytes = bytes.len(),
        "HLS 分片代理成功"
    );

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "video/MP2T")
        // TS 分片内容不变，强缓存 1 年
        .header(header::CACHE_CONTROL, "public, max-age=31536000, immutable")
        .header(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            HeaderValue::from_static("*"),
        )
        .body(Body::from(bytes))
        .map_err(internal_error)
}

