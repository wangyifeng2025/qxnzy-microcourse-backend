use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use crate::models::{
    course::{Course, CreateCourse, UpdateCourse},
    enums::CourseStatus,
    pagination::{PageQuery, PagedList},
};

/// SELECT 字段列表（含 LEFT JOIN users 取 real_name）
/// 统一在一处维护，避免各查询不一致
// const SELECT_COLS: &str = r#"
//     c.id, c.title, c.description, c.cover_image_url, c.major_id, c.teacher_id,
//     u.real_name AS teacher_name,
//     c.status AS "status: _", c.created_at, c.updated_at
// "#;

/// 门户/首页：仅已发布课程
pub async fn find_all_published(
    pool: &PgPool,
    query: &PageQuery,
) -> Result<PagedList<Course>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_created_at), Some(cursor_id)) => {
            sqlx::query_as!(
                Course,
                r#"
                SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
                       c.teacher_id, u.real_name AS teacher_name,
                       c.status AS "status: _", c.created_at, c.updated_at,
                       c.vote_count
                FROM courses c
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE c.status = 'published'::course_status
                  AND (c.created_at, c.id) < ($1, $2)
                ORDER BY c.created_at DESC, c.id DESC
                LIMIT $3
                "#,
                cursor_created_at,
                cursor_id,
                fetch_limit,
            )
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as!(
                Course,
                r#"
                SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
                       c.teacher_id, u.real_name AS teacher_name,
                       c.status AS "status: _", c.created_at, c.updated_at,
                       c.vote_count
                FROM courses c
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE c.status = 'published'::course_status
                ORDER BY c.created_at DESC, c.id DESC
                LIMIT $1
                "#,
                fetch_limit,
            )
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
            .map(|c| (Some(c.created_at), Some(c.id)))
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

/// 课程管理列表：`teacher_filter = None` 时管理员看全站；`Some(teacher_id)` 时仅该教师课程（含草稿/归档）
pub async fn find_all_managed(
    pool: &PgPool,
    query: &PageQuery,
    teacher_filter: Option<Uuid>,
) -> Result<PagedList<Course>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_created_at), Some(cursor_id)) => {
            sqlx::query_as::<_, Course>(
                r#"
                SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
                       c.teacher_id, u.real_name AS teacher_name,
                       c.status, c.created_at, c.updated_at,
                       c.vote_count
                FROM courses c
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE ($4::uuid IS NULL OR c.teacher_id = $4)
                  AND (c.created_at, c.id) < ($1, $2)
                ORDER BY c.created_at DESC, c.id DESC
                LIMIT $3
                "#,
            )
            .bind(cursor_created_at)
            .bind(cursor_id)
            .bind(fetch_limit)
            .bind(teacher_filter)
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as::<_, Course>(
                r#"
                SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
                       c.teacher_id, u.real_name AS teacher_name,
                       c.status, c.created_at, c.updated_at,
                       c.vote_count
                FROM courses c
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE ($2::uuid IS NULL OR c.teacher_id = $2)
                ORDER BY c.created_at DESC, c.id DESC
                LIMIT $1
                "#,
            )
            .bind(fetch_limit)
            .bind(teacher_filter)
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
            .map(|c| (Some(c.created_at), Some(c.id)))
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

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Course>, sqlx::Error> {
    sqlx::query_as!(
        Course,
        r#"
        SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
               c.teacher_id, u.real_name AS teacher_name,
               c.status AS "status: _", c.created_at, c.updated_at,
               c.vote_count
        FROM courses c
        LEFT JOIN users u ON u.id = c.teacher_id
        WHERE c.id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &PgPool,
    teacher_id: Uuid,
    payload: &CreateCourse,
) -> Result<Course, sqlx::Error> {
    sqlx::query_as!(
        Course,
        r#"
        WITH inserted AS (
            INSERT INTO courses (title, description, cover_image_url, major_id, teacher_id)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, title, description, cover_image_url, major_id, teacher_id, status, created_at, updated_at, vote_count
        )
        SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
               c.teacher_id, u.real_name AS teacher_name,
               c.status AS "status: _", c.created_at, c.updated_at,
               c.vote_count
        FROM inserted c
        INNER JOIN users u ON u.id = c.teacher_id
        "#,
        payload.title,
        payload.description,
        payload.cover_image_url,
        payload.major_id,
        teacher_id,
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateCourse,
) -> Result<Option<Course>, sqlx::Error> {
    sqlx::query_as!(
        Course,
        r#"
        WITH updated AS (
            UPDATE courses
            SET
                title           = COALESCE($2, title),
                description     = COALESCE($3, description),
                cover_image_url = COALESCE($4, cover_image_url),
                major_id        = COALESCE($5, major_id),
                status          = COALESCE($6, status),
                updated_at      = NOW()
            WHERE id = $1
            RETURNING id, title, description, cover_image_url, major_id, teacher_id, status, created_at, updated_at, vote_count
        )
        SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
               c.teacher_id, u.real_name AS teacher_name,
               c.status AS "status: _", c.created_at, c.updated_at,
               c.vote_count
        FROM updated c
        INNER JOIN users u ON u.id = c.teacher_id
        "#,
        id,
        payload.title,
        payload.description,
        payload.cover_image_url,
        payload.major_id,
        payload.status.clone() as Option<CourseStatus>,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM courses WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}

/// 设置课程封面 object key（MinIO 路径，非完整 URL）
pub async fn set_cover_image_url(pool: &PgPool, id: Uuid, key: &str) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE courses SET cover_image_url = $2, updated_at = NOW() WHERE id = $1"#)
        .bind(id)
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

/// 清除课程封面字段
pub async fn clear_cover_image_url(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE courses SET cover_image_url = NULL, updated_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 确认上传后仅刷新 `updated_at`
pub async fn touch_course_updated(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE courses SET updated_at = NOW() WHERE id = $1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// 切换课程投票状态（原子化，无竞态条件）
/// 返回 (voted: bool, vote_count: i64)：操作后的投票状态和最新总票数
pub async fn toggle_vote(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<(bool, i64), sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 原子尝试插入；利用 UNIQUE 约束，成功插入 rows_affected=1，冲突则为 0
    let inserted = sqlx::query!(
        r#"
        INSERT INTO course_votes (user_id, course_id)
        VALUES ($1, $2)
        ON CONFLICT (user_id, course_id) DO NOTHING
        "#,
        user_id,
        course_id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    let vote_count = if inserted == 1 {
        // 首次投票：+1
        sqlx::query_scalar!(
            r#"UPDATE courses SET vote_count = vote_count + 1 WHERE id = $1 RETURNING vote_count"#,
            course_id,
        )
        .fetch_one(&mut *tx)
        .await?
    } else {
        // 已投票，取消：DELETE + -1
        sqlx::query!(
            r#"DELETE FROM course_votes WHERE user_id = $1 AND course_id = $2"#,
            user_id,
            course_id,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query_scalar!(
            r#"UPDATE courses SET vote_count = GREATEST(0, vote_count - 1) WHERE id = $1 RETURNING vote_count"#,
            course_id,
        )
        .fetch_one(&mut *tx)
        .await?
    };

    tx.commit().await?;
    Ok((inserted == 1, vote_count))
}

/// 批量查询用户在指定课程列表中已投票的课程 ID 集合（用于列表接口避免 N+1）
pub async fn batch_get_voted_courses(
    pool: &PgPool,
    user_id: Uuid,
    course_ids: &[Uuid],
) -> Result<HashSet<Uuid>, sqlx::Error> {
    if course_ids.is_empty() {
        return Ok(HashSet::new());
    }
    let rows = sqlx::query_scalar!(
        r#"
        SELECT course_id
        FROM course_votes
        WHERE user_id = $1
          AND course_id = ANY($2::uuid[])
        "#,
        user_id,
        course_ids as &[Uuid],
    )
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().collect())
}
