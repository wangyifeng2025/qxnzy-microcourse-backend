use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    Extension, Json,
};
use sqlx::PgPool;

use crate::models::discover::{
    ActiveTeacherResponse, DiscoverLimitQuery, LatestTopicResponse, PopularCourseResponse,
};
use crate::repositories::discover as discover_repo;
use crate::storage::AppStorage;

type AppResult<T> = Result<Json<T>, (StatusCode, String)>;

fn internal_error(e: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// 将课程封面字段转为可访问 URL（MinIO key → 预签名；已是 http(s) 则原样）。
async fn resolve_cover(stored: Option<String>, storage: &AppStorage) -> Option<String> {
    let s = stored?;
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if s.starts_with("http://") || s.starts_with("https://") {
        return Some(s.to_string());
    }
    const TTL_SECS: u64 = 3600 * 24 * 7;
    match storage.presigned_get_url(s, TTL_SECS).await {
        Ok(url) => Some(url),
        Err(e) => {
            tracing::warn!(error = %e, key = %s, "发现页：课程封面预签名失败");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/discover/popular-courses?limit=10
// ---------------------------------------------------------------------------

/// 热门课程列表。
///
/// 评分公式：`enrollment_count * 2 + vote_count`
/// - 选课人数权重更高，因为选课需要主动决策，比"点赞"更有参考价值。
/// - 只统计已发布课程，按得分从高到低取前 `limit` 条（最大 50）。
///
/// 无需登录。
pub async fn popular_courses(
    State(pool): State<PgPool>,
    Extension(storage): Extension<Arc<AppStorage>>,
    Query(query): Query<DiscoverLimitQuery>,
) -> AppResult<Vec<PopularCourseResponse>> {
    let rows = discover_repo::popular_courses(&pool, query.limit())
        .await
        .map_err(internal_error)?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let cover_image_url = resolve_cover(row.cover_image_url, &storage).await;
        result.push(PopularCourseResponse {
            id: row.id,
            title: row.title,
            description: row.description,
            cover_image_url,
            major_id: row.major_id,
            major_name: row.major_name,
            teacher_id: row.teacher_id,
            teacher_name: row.teacher_name,
            status: row.status,
            created_at: row.created_at,
            updated_at: row.updated_at,
            vote_count: row.vote_count,
            enrollment_count: row.enrollment_count,
        });
    }

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// GET /api/discover/active-teachers?limit=10
// ---------------------------------------------------------------------------

/// 活跃教师列表。
///
/// 判定标准（三级排序）：
/// 1. `total_student_count`：旗下所有已发布课程的不重复学生总数（广度与影响力）
/// 2. `recent_activity_count`：近 30 天在各社区（课程 + 独立）发表的话题数 + 回复数（近期活跃度）
/// 3. `published_course_count`：已发布课程数（兜底）
///
/// 准入条件：至少 1 门已发布课程 + 账号已启用。
///
/// 无需登录。
pub async fn active_teachers(
    State(pool): State<PgPool>,
    Query(query): Query<DiscoverLimitQuery>,
) -> AppResult<Vec<ActiveTeacherResponse>> {
    let result = discover_repo::active_teachers(&pool, query.limit())
        .await
        .map_err(internal_error)?;

    Ok(Json(result))
}

// ---------------------------------------------------------------------------
// GET /api/discover/latest-topics?limit=10
// ---------------------------------------------------------------------------

/// 最新话题列表。
///
/// 汇总「课程社区话题」和「独立社区话题」，按发布时间倒序取前 `limit` 条。
/// 响应中的 `source` 字段区分来源：
/// - `"course"`    → `source_id` 为课程 ID，`source_title` 为课程标题
/// - `"community"` → `source_id` / `source_title` 均为 null
/// `content_preview` 为正文前 200 个字符。
///
/// 无需登录。
pub async fn latest_topics(
    State(pool): State<PgPool>,
    Query(query): Query<DiscoverLimitQuery>,
) -> AppResult<Vec<LatestTopicResponse>> {
    let result = discover_repo::latest_topics(&pool, query.limit())
        .await
        .map_err(internal_error)?;

    Ok(Json(result))
}
