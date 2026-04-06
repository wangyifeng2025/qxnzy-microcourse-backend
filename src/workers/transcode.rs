/// 视频转码后台 Worker
///
/// 架构说明：
/// - PostgreSQL video_transcodes 表作为持久化任务队列
/// - `claim_pending_transcode_batch` 一次认领同一视频的所有 pending 分辨率任务
/// - 单次多路输出 FFmpeg（filter_complex split）：只解码一次，产出所有清晰度
/// - HLS 分片并发上传到 MinIO（JoinSet）
/// - 多个并发 Worker goroutine（FOR UPDATE SKIP LOCKED 保证不重复认领）
///
/// 任务状态流转：pending → processing → done / failed
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use tokio::process::Command;
use tokio::task::JoinSet;

use crate::models::enums::VideoStatus;
use crate::models::video::VideoTranscode;
use crate::repositories::video as video_repo;
use crate::storage::AppStorage;
use crate::utils::ffprobe;

/// 同时运行的 Worker 数量（每个 Worker 独立认领一批视频任务）
const WORKER_COUNT: usize = 2;

/// 启动转码 Worker 主循环（在 main 中 `tokio::spawn` 调用）
pub async fn run(pool: PgPool, storage: Arc<AppStorage>) {
    tracing::info!(worker_count = WORKER_COUNT, "转码 Worker 已启动");
    let pool = Arc::new(pool);
    let mut handles = Vec::with_capacity(WORKER_COUNT);

    for worker_id in 0..WORKER_COUNT {
        let pool = Arc::clone(&pool);
        let storage = Arc::clone(&storage);
        handles.push(tokio::spawn(async move {
            worker_loop(worker_id, pool, storage).await;
        }));
    }

    for handle in handles {
        if let Err(e) = handle.await {
            tracing::error!(error = %e, "Worker goroutine 意外终止");
        }
    }
}

async fn worker_loop(worker_id: usize, pool: Arc<PgPool>, storage: Arc<AppStorage>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        match process_next_batch(&pool, &storage).await {
            Ok(true) => {}
            Ok(false) => {}
            Err(e) => tracing::error!(worker_id, error = %e, "转码任务处理失败"),
        }
    }
}

/// 认领并处理一批（同一视频的所有 pending 分辨率）转码任务
async fn process_next_batch(
    pool: &PgPool,
    storage: &Arc<AppStorage>,
) -> Result<bool, Box<dyn std::error::Error + Send + Sync>> {
    let tasks = video_repo::claim_pending_transcode_batch(pool).await?;
    if tasks.is_empty() {
        return Ok(false);
    }

    let video_id = tasks[0].video_id;
    let task_ids: Vec<uuid::Uuid> = tasks.iter().map(|t| t.id).collect();
    let resolutions: Vec<&str> = tasks.iter().map(|t| t.resolution.as_str()).collect();

    tracing::info!(
        video_id = %video_id,
        resolutions = ?resolutions,
        "开始批量转码"
    );

    let video = video_repo::find_by_id(pool, video_id)
        .await?
        .ok_or_else(|| format!("视频记录不存在：{video_id}"))?;

    let original_key = video
        .original_url
        .as_deref()
        .ok_or_else(|| format!("视频 {video_id} 尚无 original_url"))?
        .to_string();

    let tmp_dir = tempfile::Builder::new()
        .prefix(&format!("transcode_{video_id}_"))
        .tempdir()?;
    let work_dir = tmp_dir.path().to_path_buf();

    let result = transcode_batch(
        pool,
        storage,
        video_id,
        &tasks,
        &original_key,
        video.cover_url.is_none(),
        &work_dir,
    )
    .await;

    match result {
        Ok(()) => {
            tracing::info!(video_id = %video_id, resolutions = ?resolutions, "批量转码完成");

            let all_done = video_repo::all_transcodes_done(pool, video_id).await?;
            if all_done {
                video_repo::update_status(pool, video_id, VideoStatus::Ready).await?;
                tracing::info!(video_id = %video_id, "视频所有分辨率转码完成，状态更新为 ready");
            } else {
                let has_failed = video_repo::has_failed_transcode(pool, video_id).await?;
                if has_failed {
                    video_repo::update_status(pool, video_id, VideoStatus::Failed).await?;
                    tracing::warn!(video_id = %video_id, "存在失败的转码任务，状态更新为 failed");
                }
            }
        }
        Err(e) => {
            tracing::error!(video_id = %video_id, error = %e, "批量转码失败");
            for task_id in &task_ids {
                video_repo::fail_transcode(pool, *task_id).await?;
            }
            video_repo::update_status(pool, video_id, VideoStatus::Failed).await?;
        }
    }

    Ok(true)
}

// ---------------------------------------------------------------------------
// 核心转码流程
// ---------------------------------------------------------------------------

async fn transcode_batch(
    pool: &PgPool,
    storage: &Arc<AppStorage>,
    video_id: uuid::Uuid,
    tasks: &[VideoTranscode],
    original_key: &str,
    need_thumbnail: bool,
    work_dir: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 步骤 1：从 MinIO 下载原始视频（只下载一次，所有分辨率共用）
    let ext = Path::new(original_key)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("mp4");
    let input_path = work_dir.join(format!("input.{ext}"));

    tracing::debug!(key = %original_key, "从对象存储下载原始视频");
    let video_bytes = storage.download_bytes(original_key).await?;
    tokio::fs::write(&input_path, &video_bytes).await?;
    drop(video_bytes);

    // 对已落盘的本地文件探测时长（confirm 若未成功 ffprobe，此处兜底修正错误/缺失的 duration）
    if let Some(input_str) = input_path.to_str() {
        match ffprobe::probe_duration_seconds(input_str).await {
            Ok(secs) => {
                if let Err(e) = video_repo::update_duration(pool, video_id, secs).await {
                    tracing::warn!(error = %e, video_id = %video_id, "更新视频时长失败");
                } else {
                    tracing::debug!(video_id = %video_id, duration_secs = secs, "转码前 ffprobe 写入时长");
                }
            }
            Err(e) => {
                tracing::debug!(error = %e, video_id = %video_id, "转码阶段 ffprobe 探测时长失败")
            }
        }
    }

    // 步骤 2：提取封面（仅快速 seek 单帧，时间可忽略不计）
    if need_thumbnail {
        let cover_path = work_dir.join("thumbnail.jpg");
        match extract_thumbnail(&input_path, &cover_path).await {
            Err(e) => tracing::warn!(error = %e, "封面截图提取失败，跳过"),
            Ok(()) => {
                let cover_key = format!("covers/{video_id}/thumbnail.jpg");
                let cover_data = tokio::fs::read(&cover_path).await?;
                storage
                    .upload_bytes(&cover_key, cover_data, "image/jpeg")
                    .await?;
                video_repo::update_cover_url(pool, video_id, &cover_key).await?;
                tracing::info!(video_id = %video_id, key = %cover_key, "封面上传成功");
            }
        }
    }

    // 步骤 3：收集本批次需要转码的分辨率
    let res_list: Vec<(&str, u32)> = tasks
        .iter()
        .filter_map(|t| resolution_to_height(&t.resolution).map(|h| (t.resolution.as_str(), h)))
        .collect();

    for (res, _) in &res_list {
        tokio::fs::create_dir_all(work_dir.join(res)).await?;
    }

    // 步骤 4：单次多路输出 FFmpeg（filter_complex split，只解码一次）
    //
    // veryfast preset 比 medium 快约 3×，对流媒体质量影响极小（同 CRF）
    tracing::debug!(resolutions = ?res_list.iter().map(|(r,_)| r).collect::<Vec<_>>(), "启动 FFmpeg 多路转码");
    run_ffmpeg_multiresolution(&input_path, work_dir, &res_list).await?;

    // 步骤 5：并发上传所有 HLS 分片到 MinIO
    //
    // 收集全部 (file_path, object_key, content_type, resolution) 元组，
    // 然后用 JoinSet 并发上传，避免串行 I/O 等待。
    let mut all_files: Vec<(PathBuf, String, &'static str, String)> = Vec::new();
    for task in tasks {
        let hls_prefix = format!("hls/{video_id}/{}", task.resolution);
        let output_dir = work_dir.join(&task.resolution);
        let mut entries = tokio::fs::read_dir(&output_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let content_type = if filename.ends_with(".m3u8") {
                "application/x-mpegURL"
            } else {
                "video/MP2T"
            };
            all_files.push((
                path,
                format!("{hls_prefix}/{filename}"),
                content_type,
                task.resolution.clone(),
            ));
        }
    }

    let mut upload_set: JoinSet<Result<(String, i64), String>> = JoinSet::new();
    for (file_path, object_key, content_type, resolution) in all_files {
        let storage_clone = Arc::clone(storage);
        upload_set.spawn(async move {
            let data = tokio::fs::read(&file_path)
                .await
                .map_err(|e| e.to_string())?;
            let size = data.len() as i64;
            storage_clone
                .upload_bytes(&object_key, data, content_type)
                .await?;
            Ok::<(String, i64), String>((resolution, size))
        });
    }

    let mut resolution_sizes: HashMap<String, i64> = HashMap::new();
    while let Some(join_result) = upload_set.join_next().await {
        let (resolution, size) = join_result.map_err(|e| e.to_string())?.map_err(|e| e)?;
        *resolution_sizes.entry(resolution).or_default() += size;
    }

    // 步骤 6：更新各分辨率转码任务状态为 done
    for task in tasks {
        let total_size = resolution_sizes.get(&task.resolution).copied().unwrap_or(0);
        let playlist_key = format!("hls/{video_id}/{}/playlist.m3u8", task.resolution);
        video_repo::complete_transcode(pool, task.id, &playlist_key, total_size).await?;
        tracing::info!(
            task_id = %task.id,
            resolution = %task.resolution,
            total_size_bytes = total_size,
            "HLS 上传完成"
        );
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// FFmpeg 调用封装
// ---------------------------------------------------------------------------

/// 单次多路输出 FFmpeg：使用 `filter_complex split` 只解码一次，输出所有目标分辨率的 HLS。
///
/// 生成命令示例（4 路）：
/// ```
/// ffmpeg -y -i input.mp4
///   -filter_complex "[0:v]split=4[v0][v1][v2][v3];[v0]scale=-2:1080[s0];..."
///   -map [s0] -map 0:a? -c:v libx264 -preset veryfast -crf 23 -c:a aac -b:a 128k
///      -hls_time 10 -hls_playlist_type vod -hls_segment_filename 1080p/seg_%04d.ts 1080p/playlist.m3u8
///   -map [s1] ... 720p/playlist.m3u8
///   ...
/// ```
async fn run_ffmpeg_multiresolution(
    input: &Path,
    work_dir: &Path,
    resolutions: &[(&str, u32)],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let n = resolutions.len();
    assert!(n > 0, "至少需要一个分辨率");

    // 构建 filter_complex
    let filter_complex = if n == 1 {
        let (_, h) = resolutions[0];
        format!("[0:v]scale=-2:{h}[s0]")
    } else {
        let split_labels: String = (0..n).map(|i| format!("[v{i}]")).collect();
        let scale_chains: String = (0..n)
            .map(|i| {
                let (_, h) = resolutions[i];
                format!("[v{i}]scale=-2:{h}[s{i}]")
            })
            .collect::<Vec<_>>()
            .join(";");
        format!("[0:v]split={n}{split_labels};{scale_chains}")
    };

    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.to_str().unwrap().to_string(),
        "-filter_complex".into(),
        filter_complex,
    ];

    for (i, (res_name, _)) in resolutions.iter().enumerate() {
        let output_dir = work_dir.join(res_name);
        let playlist = output_dir.join("playlist.m3u8");
        let segments = output_dir.join("segment_%04d.ts");

        args.extend([
            "-map".to_string(),
            format!("[s{i}]"),
            "-map".into(),
            "0:a?".into(),
            "-c:v".into(),
            "libx264".into(),
            "-preset".into(),
            "veryfast".into(),
            "-crf".into(),
            "23".into(),
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            "128k".into(),
            "-hls_time".into(),
            "10".into(),
            "-hls_playlist_type".into(),
            "vod".into(),
            "-hls_segment_filename".into(),
            segments.to_str().unwrap().to_string(),
            playlist.to_str().unwrap().to_string(),
        ]);
    }

    let output = Command::new("ffmpeg").args(&args).output().await?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 多路转码失败：{stderr}").into());
    }

    Ok(())
}

/// 调用 FFmpeg 从视频第 3 秒提取一帧作为封面缩略图
///
/// `-ss` 放在 `-i` 之前（输入 seek），几乎瞬间完成，不需要解码整个视频。
async fn extract_thumbnail(
    input: &Path,
    output: &Path,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let result = Command::new("ffmpeg")
        .args([
            "-y",
            "-ss",
            "00:00:03",
            "-i",
            input.to_str().unwrap(),
            "-vframes",
            "1",
            "-q:v",
            "2",
            output.to_str().unwrap(),
        ])
        .output()
        .await?;

    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(format!("FFmpeg 截图失败：{stderr}").into());
    }

    Ok(())
}

/// 将分辨率字符串转换为目标高度（像素）
fn resolution_to_height(resolution: &str) -> Option<u32> {
    match resolution {
        "1080p" => Some(1080),
        "720p" => Some(720),
        "480p" => Some(480),
        "360p" => Some(360),
        _ => None,
    }
}
