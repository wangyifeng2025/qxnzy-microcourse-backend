use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    major::{CreateMajor, Major, MajorWithStats, UpdateMajor},
    pagination::{PageQuery, PagedList},
};

/// 带统计的 SELECT（子查询聚合，单次查询，无 N+1）
const STATS_SELECT: &str = r#"
SELECT
    m.id, m.name, m.code, m.description, m.sort_order, m.created_at,
    COALESCE(cs.course_count, 0)           AS course_count,
    COALESCE(ls.enrolled_learner_count, 0) AS enrolled_learner_count,
    COALESCE(vs.total_video_views, 0)      AS total_video_views
FROM majors m
LEFT JOIN (
    SELECT major_id, COUNT(*) AS course_count
    FROM courses
    WHERE status = 'published'
    GROUP BY major_id
) cs ON cs.major_id = m.id
LEFT JOIN (
    SELECT c.major_id, COUNT(DISTINCT ce.user_id) AS enrolled_learner_count
    FROM course_enrollments ce
    JOIN courses c ON c.id = ce.course_id
    GROUP BY c.major_id
) ls ON ls.major_id = m.id
LEFT JOIN (
    SELECT c.major_id, SUM(v.view_count) AS total_video_views
    FROM videos v
    JOIN chapters ch ON ch.id = v.chapter_id
    JOIN courses c   ON c.id  = ch.course_id
    GROUP BY c.major_id
) vs ON vs.major_id = m.id
"#;

pub async fn find_all(
    pool: &PgPool,
    query: &PageQuery,
) -> Result<PagedList<MajorWithStats>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_created_at), Some(cursor_id)) => {
            sqlx::query_as::<_, MajorWithStats>(&format!(
                "{} WHERE (m.created_at, m.id) < ($1, $2) ORDER BY m.created_at DESC, m.id DESC LIMIT $3",
                STATS_SELECT.trim()
            ))
            .bind(cursor_created_at)
            .bind(cursor_id)
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as::<_, MajorWithStats>(&format!(
                "{} ORDER BY m.created_at DESC, m.id DESC LIMIT $1",
                STATS_SELECT.trim()
            ))
            .bind(fetch_limit)
            .fetch_all(pool)
            .await?
        }
    };

    let has_more = items.len() as i64 > page_size;
    if has_more {
        items.truncate(page_size as usize);
    }

    let (next_cursor_created_at, next_cursor_id) = if has_more {
        items
            .last()
            .map(|m| (Some(m.created_at), Some(m.id)))
            .unwrap_or((None, None))
    } else {
        (None, None)
    };

    Ok(PagedList {
        page_size,
        has_more,
        next_cursor_created_at,
        next_cursor_id,
        items,
    })
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<MajorWithStats>, sqlx::Error> {
    sqlx::query_as::<_, MajorWithStats>(&format!(
        "{} WHERE m.id = $1",
        STATS_SELECT.trim()
    ))
    .bind(id)
    .fetch_optional(pool)
    .await
}

pub async fn create(pool: &PgPool, payload: &CreateMajor) -> Result<Major, sqlx::Error> {
    sqlx::query_as!(
        Major,
        r#"
        INSERT INTO majors (name, code, description, sort_order)
        VALUES ($1, $2, $3, COALESCE($4, 0))
        RETURNING id, name, code, description, sort_order, created_at
        "#,
        payload.name,
        payload.code,
        payload.description,
        payload.sort_order,
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateMajor,
) -> Result<Option<Major>, sqlx::Error> {
    sqlx::query_as!(
        Major,
        r#"
        UPDATE majors
        SET
            name = COALESCE($2, name),
            code = COALESCE($3, code),
            description = COALESCE($4, description),
            sort_order = COALESCE($5, sort_order)
        WHERE id = $1
        RETURNING id, name, code, description, sort_order, created_at
        "#,
        id,
        payload.name,
        payload.code,
        payload.description,
        payload.sort_order,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM majors WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}
