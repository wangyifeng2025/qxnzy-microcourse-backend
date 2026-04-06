use tokio::process::Command;

/// 对本地路径或 HTTP(S) URL 探测时长，四舍五入为整秒（至少为 1 秒，避免 0 秒占位）。
///
/// 由 `handlers::video::confirm_upload` 与 `workers::transcode` 调用。
pub(crate) async fn probe_duration_seconds(input: &str) -> Result<i32, String> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
            input,
        ])
        .output()
        .await
        .map_err(|e| format!("无法执行 ffprobe: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("ffprobe 失败: {stderr}"));
    }

    let s = String::from_utf8_lossy(&output.stdout);
    let trimmed = s.trim();
    let duration_f: f64 = trimmed
        .parse()
        .map_err(|_| format!("无法解析时长输出: {trimmed:?}"))?;

    if !duration_f.is_finite() || duration_f <= 0.0 {
        return Err(format!("无效时长值: {duration_f}"));
    }

    let secs = duration_f.round() as i32;
    Ok(secs.max(1))
}
