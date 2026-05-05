use sqlx::PgPool;
use uuid::Uuid;

use crate::models::community::{
    CourseMember, CommunityTopicMember, CommunityTopicReplyResponse, CommunityTopicResponse,
    CourseTopicReplyResponse, CourseTopicResponse, TopicPageQuery,
};
use crate::models::pagination::{PageQuery, PagedList};

// ---------------------------------------------------------------------------
// 权限辅助
// ---------------------------------------------------------------------------

/// 判断用户是否为课程社区成员（已选课学生 **或** 课程教师 **或** 管理员）。
/// 注意：调用方须先用 `course_repo::find_by_id` 确认课程存在。
pub async fn is_community_member(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<bool, sqlx::Error> {
    // 教师判断放在 handler 层（已有 course.teacher_id），此处仅检查选课记录
    let enrolled = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM course_enrollments WHERE user_id = $1 AND course_id = $2"#,
        user_id,
        course_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(enrolled > 0)
}

// ---------------------------------------------------------------------------
// 话题（Topic）
// ---------------------------------------------------------------------------

/// 分页查询话题列表（置顶优先，然后 created_at DESC）。
///
/// 游标编码：`(cursor_is_pinned, cursor_created_at, cursor_id)` 三元组，
/// 对应排序键 `(is_pinned DESC, created_at DESC, id DESC)`。
/// 下一页游标由 `TopicPageQuery` 额外携带 `cursor_is_pinned` 字段。
pub async fn list_topics(
    pool: &PgPool,
    course_id: Uuid,
    query: &TopicPageQuery,
) -> Result<TopicPagedList, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items: Vec<CourseTopicResponse> = match (query.cursor_is_pinned, query.cursor_created_at, query.cursor_id) {
        (Some(cursor_is_pinned), Some(cursor_created_at), Some(cursor_id)) => {
            // 排序 (is_pinned DESC, created_at DESC, id DESC)：
            // "比游标小"等价于：
            //   is_pinned < cursor_is_pinned   (FALSE < TRUE，即从置顶跨到普通)
            //   OR (is_pinned = cursor_is_pinned AND created_at < cursor_created_at)
            //   OR (is_pinned = cursor_is_pinned AND created_at = cursor_created_at AND id < cursor_id)
            sqlx::query_as!(
                CourseTopicResponse,
                r#"
                SELECT
                    t.id, t.course_id, t.author_id,
                    u.username   AS author_username,
                    u.real_name  AS author_real_name,
                    u.avatar_url AS author_avatar_url,
                    u.role       AS "author_role: _",
                    t.title, t.content, t.is_pinned, t.reply_count,
                    t.created_at, t.updated_at
                FROM course_topics t
                JOIN users u ON u.id = t.author_id
                WHERE t.course_id = $1
                  AND (
                    (t.is_pinned = FALSE AND $2 = TRUE)
                    OR (t.is_pinned = $2 AND t.created_at < $3)
                    OR (t.is_pinned = $2 AND t.created_at = $3 AND t.id < $4)
                  )
                ORDER BY t.is_pinned DESC, t.created_at DESC, t.id DESC
                LIMIT $5
                "#,
                course_id,
                cursor_is_pinned,
                cursor_created_at,
                cursor_id,
                fetch_limit,
            )
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as!(
                CourseTopicResponse,
                r#"
                SELECT
                    t.id, t.course_id, t.author_id,
                    u.username   AS author_username,
                    u.real_name  AS author_real_name,
                    u.avatar_url AS author_avatar_url,
                    u.role       AS "author_role: _",
                    t.title, t.content, t.is_pinned, t.reply_count,
                    t.created_at, t.updated_at
                FROM course_topics t
                JOIN users u ON u.id = t.author_id
                WHERE t.course_id = $1
                ORDER BY t.is_pinned DESC, t.created_at DESC, t.id DESC
                LIMIT $2
                "#,
                course_id,
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
    let next_cursor = if has_more {
        items.last().map(|t| TopicCursor {
            created_at: t.created_at,
            id: t.id,
            is_pinned: t.is_pinned,
        })
    } else {
        None
    };

    Ok(TopicPagedList {
        page_size,
        has_more,
        next_cursor,
        items,
    })
}

/// 话题列表专用分页响应（包含 `is_pinned` 游标字段）
#[derive(Debug, serde::Serialize)]
pub struct TopicCursor {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub id: uuid::Uuid,
    pub is_pinned: bool,
}

/// 话题分页列表响应
#[derive(Debug, serde::Serialize)]
pub struct TopicPagedList {
    pub page_size: i64,
    pub has_more: bool,
    /// `None` 表示已无更多数据；非 `None` 时将三个字段原样传回下一次请求
    pub next_cursor: Option<TopicCursor>,
    pub items: Vec<CourseTopicResponse>,
}

#[derive(Debug, sqlx::FromRow)]
struct CourseTopicHighlightRow {
    id: Uuid,
    title: String,
    content_preview: Option<String>,
    reply_count: i32,
    author_username: String,
    author_real_name: Option<String>,
}

/// 已发布课程的讨论精选：置顶优先，其次回复数、发布时间。仅用于公开页展示。
pub async fn list_topic_highlights_public(
    pool: &PgPool,
    course_id: Uuid,
    limit: i64,
) -> Result<Vec<crate::models::community::CourseTopicHighlightResponse>, sqlx::Error> {
    let rows = sqlx::query_as::<_, CourseTopicHighlightRow>(
        r#"
        SELECT
            t.id,
            t.title,
            LEFT(t.content, 180) AS content_preview,
            t.reply_count,
            u.username AS author_username,
            u.real_name AS author_real_name
        FROM course_topics t
        JOIN users u ON u.id = t.author_id
        WHERE t.course_id = $1
        ORDER BY t.is_pinned DESC, t.reply_count DESC, t.created_at DESC
        LIMIT $2
        "#,
    )
    .bind(course_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let author_display = r
                .author_real_name
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .unwrap_or_else(|| r.author_username.clone());
            crate::models::community::CourseTopicHighlightResponse {
                id: r.id,
                title: r.title,
                content_preview: r.content_preview.unwrap_or_default(),
                reply_count: r.reply_count,
                author_display,
            }
        })
        .collect())
}

/// 通过 ID 查询单条话题（含作者信息）
pub async fn find_topic_by_id(
    pool: &PgPool,
    topic_id: Uuid,
) -> Result<Option<CourseTopicResponse>, sqlx::Error> {
    sqlx::query_as!(
        CourseTopicResponse,
        r#"
        SELECT
            t.id, t.course_id, t.author_id,
            u.username   AS author_username,
            u.real_name  AS author_real_name,
            u.avatar_url AS author_avatar_url,
            u.role       AS "author_role: _",
            t.title, t.content, t.is_pinned, t.reply_count,
            t.created_at, t.updated_at
        FROM course_topics t
        JOIN users u ON u.id = t.author_id
        WHERE t.id = $1
        "#,
        topic_id,
    )
    .fetch_optional(pool)
    .await
}

/// 创建话题，同时在同一事务中插入 @提及记录。
/// 返回完整的话题响应（含作者信息）。
pub async fn create_topic(
    pool: &PgPool,
    course_id: Uuid,
    author_id: Uuid,
    title: &str,
    content: &str,
    mention_user_ids: &[Uuid],
) -> Result<CourseTopicResponse, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let topic_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO course_topics (id, course_id, author_id, title, content, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, $2, $3, $4, NOW(), NOW())
        RETURNING id
        "#,
        course_id,
        author_id,
        title,
        content,
    )
    .fetch_one(&mut *tx)
    .await?;

    for &mentioned_id in mention_user_ids {
        sqlx::query!(
            r#"
            INSERT INTO course_topic_mentions (id, topic_id, mentioned_user_id, created_at)
            VALUES (gen_random_uuid(), $1, $2, NOW())
            "#,
            topic_id,
            mentioned_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_topic_by_id(pool, topic_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除话题（级联删除回复和提及记录由数据库 ON DELETE CASCADE 保证）。
/// 返回是否实际删除了记录。
pub async fn delete_topic(pool: &PgPool, topic_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"DELETE FROM course_topics WHERE id = $1"#,
        topic_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// 回复（Reply）
// ---------------------------------------------------------------------------

/// 分页查询话题下的回复（时间正序，越早越靠前）
pub async fn list_replies(
    pool: &PgPool,
    topic_id: Uuid,
    query: &PageQuery,
) -> Result<PagedList<CourseTopicReplyResponse>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items: Vec<CourseTopicReplyResponse> = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_created_at), Some(cursor_id)) => {
            sqlx::query_as!(
                CourseTopicReplyResponse,
                r#"
                SELECT
                    r.id, r.topic_id, r.author_id,
                    u.username   AS author_username,
                    u.real_name  AS author_real_name,
                    u.avatar_url AS author_avatar_url,
                    u.role       AS "author_role: _",
                    r.content, r.reply_to_reply_id,
                    r.created_at, r.updated_at
                FROM course_topic_replies r
                JOIN users u ON u.id = r.author_id
                WHERE r.topic_id = $1
                  AND (r.created_at, r.id) > ($2, $3)
                ORDER BY r.created_at ASC, r.id ASC
                LIMIT $4
                "#,
                topic_id,
                cursor_created_at,
                cursor_id,
                fetch_limit,
            )
            .fetch_all(pool)
            .await?
        }
        _ => {
            sqlx::query_as!(
                CourseTopicReplyResponse,
                r#"
                SELECT
                    r.id, r.topic_id, r.author_id,
                    u.username   AS author_username,
                    u.real_name  AS author_real_name,
                    u.avatar_url AS author_avatar_url,
                    u.role       AS "author_role: _",
                    r.content, r.reply_to_reply_id,
                    r.created_at, r.updated_at
                FROM course_topic_replies r
                JOIN users u ON u.id = r.author_id
                WHERE r.topic_id = $1
                ORDER BY r.created_at ASC, r.id ASC
                LIMIT $2
                "#,
                topic_id,
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
            .map(|r| (Some(r.created_at), Some(r.id)))
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

/// 通过 ID 查询单条回复（含作者信息）
pub async fn find_reply_by_id(
    pool: &PgPool,
    reply_id: Uuid,
) -> Result<Option<CourseTopicReplyResponse>, sqlx::Error> {
    sqlx::query_as!(
        CourseTopicReplyResponse,
        r#"
        SELECT
            r.id, r.topic_id, r.author_id,
            u.username   AS author_username,
            u.real_name  AS author_real_name,
            u.avatar_url AS author_avatar_url,
            u.role       AS "author_role: _",
            r.content, r.reply_to_reply_id,
            r.created_at, r.updated_at
        FROM course_topic_replies r
        JOIN users u ON u.id = r.author_id
        WHERE r.id = $1
        "#,
        reply_id,
    )
    .fetch_optional(pool)
    .await
}

/// 创建回复，同时递增 topic.reply_count 并插入 @提及记录（同一事务）。
pub async fn create_reply(
    pool: &PgPool,
    topic_id: Uuid,
    author_id: Uuid,
    content: &str,
    reply_to_reply_id: Option<Uuid>,
    mention_user_ids: &[Uuid],
) -> Result<CourseTopicReplyResponse, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let reply_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO course_topic_replies
            (id, topic_id, author_id, content, reply_to_reply_id, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, $2, $3, $4, NOW(), NOW())
        RETURNING id
        "#,
        topic_id,
        author_id,
        content,
        reply_to_reply_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    // 递增话题回复计数
    sqlx::query!(
        r#"UPDATE course_topics SET reply_count = reply_count + 1, updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    for &mentioned_id in mention_user_ids {
        sqlx::query!(
            r#"
            INSERT INTO course_topic_mentions (id, reply_id, mentioned_user_id, created_at)
            VALUES (gen_random_uuid(), $1, $2, NOW())
            "#,
            reply_id,
            mentioned_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_reply_by_id(pool, reply_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除回复，同时递减 topic.reply_count（同一事务）。
/// 返回是否实际删除了记录。
pub async fn delete_reply(pool: &PgPool, reply_id: Uuid) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 先取出 topic_id
    let maybe_topic_id: Option<Uuid> = sqlx::query_scalar!(
        r#"SELECT topic_id FROM course_topic_replies WHERE id = $1"#,
        reply_id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(topic_id) = maybe_topic_id else {
        tx.rollback().await?;
        return Ok(false);
    };

    sqlx::query!(
        r#"DELETE FROM course_topic_replies WHERE id = $1"#,
        reply_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"UPDATE course_topics SET reply_count = GREATEST(0, reply_count - 1), updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

// ---------------------------------------------------------------------------
// 课程社区成员（@提及候选）
// ---------------------------------------------------------------------------

/// 获取课程社区的可 @ 成员列表：已选课学生 + 课程教师。
///
/// 使用 EXISTS 子查询代替 UNION，以便 sqlx 能正确推断列的可空性。
pub async fn list_course_members(
    pool: &PgPool,
    course_id: Uuid,
) -> Result<Vec<CourseMember>, sqlx::Error> {
    sqlx::query_as!(
        CourseMember,
        r#"
        SELECT DISTINCT
            u.id          AS "user_id!",
            u.username    AS "username!",
            u.real_name,
            u.avatar_url,
            u.role        AS "role!: _"
        FROM users u
        WHERE u.is_active = TRUE
          AND (
            EXISTS (
                SELECT 1 FROM course_enrollments e
                WHERE e.user_id = u.id AND e.course_id = $1
            )
            OR
            EXISTS (
                SELECT 1 FROM courses c
                WHERE c.id = $1 AND c.teacher_id = u.id
            )
          )
        ORDER BY username ASC
        "#,
        course_id,
    )
    .fetch_all(pool)
    .await
}

/// 验证给定的用户 ID 列表中所有用户都是该课程的成员（已选课 OR 教师）。
/// 返回不合法的用户 ID 列表。
pub async fn validate_mention_users(
    pool: &PgPool,
    course_id: Uuid,
    user_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }

    // 查出合法成员（EXISTS 子查询，避免 UNION 导致 sqlx 类型推断歧义）
    let valid_ids: Vec<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT u.id AS "id!"
        FROM users u
        WHERE u.id = ANY($2)
          AND (
            EXISTS (
                SELECT 1 FROM course_enrollments e
                WHERE e.user_id = u.id AND e.course_id = $1
            )
            OR
            EXISTS (
                SELECT 1 FROM courses c
                WHERE c.id = $1 AND c.teacher_id = u.id
            )
          )
        "#,
        course_id,
        user_ids,
    )
    .fetch_all(pool)
    .await?;

    let invalid: Vec<Uuid> = user_ids
        .iter()
        .filter(|id| !valid_ids.contains(id))
        .copied()
        .collect();

    Ok(invalid)
}

// ===========================================================================
// 独立社区话题（community_topics）
// ===========================================================================

// ---------------------------------------------------------------------------
// 话题（Topic）
// ---------------------------------------------------------------------------

/// 分页查询独立社区话题列表（置顶优先，然后 created_at DESC）。
pub async fn list_standalone_topics(
    pool: &PgPool,
    query: &TopicPageQuery,
) -> Result<StandaloneTopicPagedList, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items: Vec<CommunityTopicResponse> =
        match (query.cursor_is_pinned, query.cursor_created_at, query.cursor_id) {
            (Some(cursor_is_pinned), Some(cursor_created_at), Some(cursor_id)) => {
                sqlx::query_as!(
                    CommunityTopicResponse,
                    r#"
                    SELECT
                        t.id, t.author_id,
                        u.username   AS author_username,
                        u.real_name  AS author_real_name,
                        u.avatar_url AS author_avatar_url,
                        u.role       AS "author_role: _",
                        t.title, t.content, t.is_pinned, t.reply_count, t.member_count,
                        t.created_at, t.updated_at
                    FROM community_topics t
                    JOIN users u ON u.id = t.author_id
                    WHERE (
                        (t.is_pinned = FALSE AND $1 = TRUE)
                        OR (t.is_pinned = $1 AND t.created_at < $2)
                        OR (t.is_pinned = $1 AND t.created_at = $2 AND t.id < $3)
                    )
                    ORDER BY t.is_pinned DESC, t.created_at DESC, t.id DESC
                    LIMIT $4
                    "#,
                    cursor_is_pinned,
                    cursor_created_at,
                    cursor_id,
                    fetch_limit,
                )
                .fetch_all(pool)
                .await?
            }
            _ => {
                sqlx::query_as!(
                    CommunityTopicResponse,
                    r#"
                    SELECT
                        t.id, t.author_id,
                        u.username   AS author_username,
                        u.real_name  AS author_real_name,
                        u.avatar_url AS author_avatar_url,
                        u.role       AS "author_role: _",
                        t.title, t.content, t.is_pinned, t.reply_count, t.member_count,
                        t.created_at, t.updated_at
                    FROM community_topics t
                    JOIN users u ON u.id = t.author_id
                    ORDER BY t.is_pinned DESC, t.created_at DESC, t.id DESC
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
    let next_cursor = if has_more {
        items.last().map(|t| TopicCursor {
            created_at: t.created_at,
            id: t.id,
            is_pinned: t.is_pinned,
        })
    } else {
        None
    };

    Ok(StandaloneTopicPagedList {
        page_size,
        has_more,
        next_cursor,
        items,
    })
}

/// 独立社区话题分页列表响应
#[derive(Debug, serde::Serialize)]
pub struct StandaloneTopicPagedList {
    pub page_size: i64,
    pub has_more: bool,
    pub next_cursor: Option<TopicCursor>,
    pub items: Vec<CommunityTopicResponse>,
}

/// 通过 ID 查询单条独立社区话题（含作者信息）
pub async fn find_standalone_topic_by_id(
    pool: &PgPool,
    topic_id: Uuid,
) -> Result<Option<CommunityTopicResponse>, sqlx::Error> {
    sqlx::query_as!(
        CommunityTopicResponse,
        r#"
        SELECT
            t.id, t.author_id,
            u.username   AS author_username,
            u.real_name  AS author_real_name,
            u.avatar_url AS author_avatar_url,
            u.role       AS "author_role: _",
            t.title, t.content, t.is_pinned, t.reply_count, t.member_count,
            t.created_at, t.updated_at
        FROM community_topics t
        JOIN users u ON u.id = t.author_id
        WHERE t.id = $1
        "#,
        topic_id,
    )
    .fetch_optional(pool)
    .await
}

/// 创建独立社区话题，同时自动将作者加入成员表并插入 @提及记录（同一事务）。
pub async fn create_standalone_topic(
    pool: &PgPool,
    author_id: Uuid,
    title: &str,
    content: &str,
    mention_user_ids: &[Uuid],
) -> Result<CommunityTopicResponse, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let topic_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO community_topics (id, author_id, title, content, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, $2, $3, NOW(), NOW())
        RETURNING id
        "#,
        author_id,
        title,
        content,
    )
    .fetch_one(&mut *tx)
    .await?;

    // 作者自动加入话题
    sqlx::query!(
        r#"
        INSERT INTO community_topic_members (topic_id, user_id, joined_at)
        VALUES ($1, $2, NOW())
        ON CONFLICT DO NOTHING
        "#,
        topic_id,
        author_id,
    )
    .execute(&mut *tx)
    .await?;

    for &mentioned_id in mention_user_ids {
        sqlx::query!(
            r#"
            INSERT INTO community_topic_mentions (id, topic_id, mentioned_user_id, created_at)
            VALUES (gen_random_uuid(), $1, $2, NOW())
            "#,
            topic_id,
            mentioned_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_standalone_topic_by_id(pool, topic_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除独立社区话题（级联删除由数据库保证）。
pub async fn delete_standalone_topic(pool: &PgPool, topic_id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"DELETE FROM community_topics WHERE id = $1"#,
        topic_id,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

// ---------------------------------------------------------------------------
// 成员（Member）
// ---------------------------------------------------------------------------

/// 判断用户是否为指定独立社区话题的成员。
pub async fn is_standalone_topic_member(
    pool: &PgPool,
    user_id: Uuid,
    topic_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let count = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM community_topic_members WHERE topic_id = $1 AND user_id = $2"#,
        topic_id,
        user_id,
    )
    .fetch_one(pool)
    .await?;

    Ok(count > 0)
}

/// 加入独立社区话题。返回 `true` 表示新加入，`false` 表示已是成员（幂等）。
pub async fn join_standalone_topic(
    pool: &PgPool,
    user_id: Uuid,
    topic_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let already = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "n!" FROM community_topic_members WHERE topic_id = $1 AND user_id = $2"#,
        topic_id,
        user_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    if already > 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query!(
        r#"INSERT INTO community_topic_members (topic_id, user_id, joined_at) VALUES ($1, $2, NOW())"#,
        topic_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"UPDATE community_topics SET member_count = member_count + 1, updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// 退出独立社区话题。返回 `true` 表示成功退出，`false` 表示原本不是成员。
/// 话题创建者不能退出（由调用方校验）。
pub async fn leave_standalone_topic(
    pool: &PgPool,
    user_id: Uuid,
    topic_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let result = sqlx::query!(
        r#"DELETE FROM community_topic_members WHERE topic_id = $1 AND user_id = $2"#,
        topic_id,
        user_id,
    )
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }

    sqlx::query!(
        r#"UPDATE community_topics SET member_count = GREATEST(1, member_count - 1), updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}

/// 获取独立社区话题成员列表（按加入时间正序）。
pub async fn list_standalone_topic_members(
    pool: &PgPool,
    topic_id: Uuid,
) -> Result<Vec<CommunityTopicMember>, sqlx::Error> {
    sqlx::query_as!(
        CommunityTopicMember,
        r#"
        SELECT
            u.id          AS "user_id!",
            u.username    AS "username!",
            u.real_name,
            u.avatar_url,
            u.role        AS "role!: _",
            m.joined_at   AS "joined_at!"
        FROM community_topic_members m
        JOIN users u ON u.id = m.user_id
        WHERE m.topic_id = $1
        ORDER BY m.joined_at ASC
        "#,
        topic_id,
    )
    .fetch_all(pool)
    .await
}

/// 验证给定用户 ID 列表中所有用户都是话题成员。
/// 返回不合法（非成员）的用户 ID。
pub async fn validate_standalone_topic_mention_users(
    pool: &PgPool,
    topic_id: Uuid,
    user_ids: &[Uuid],
) -> Result<Vec<Uuid>, sqlx::Error> {
    if user_ids.is_empty() {
        return Ok(vec![]);
    }

    let valid_ids: Vec<Uuid> = sqlx::query_scalar!(
        r#"
        SELECT user_id AS "id!"
        FROM community_topic_members
        WHERE topic_id = $1 AND user_id = ANY($2)
        "#,
        topic_id,
        user_ids,
    )
    .fetch_all(pool)
    .await?;

    let invalid: Vec<Uuid> = user_ids
        .iter()
        .filter(|id| !valid_ids.contains(id))
        .copied()
        .collect();

    Ok(invalid)
}

// ---------------------------------------------------------------------------
// 回复（Reply）
// ---------------------------------------------------------------------------

/// 分页查询独立社区话题回复（时间正序）。
pub async fn list_standalone_topic_replies(
    pool: &PgPool,
    topic_id: Uuid,
    query: &PageQuery,
) -> Result<PagedList<CommunityTopicReplyResponse>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items: Vec<CommunityTopicReplyResponse> =
        match (query.cursor_created_at, query.cursor_id) {
            (Some(cursor_created_at), Some(cursor_id)) => {
                sqlx::query_as!(
                    CommunityTopicReplyResponse,
                    r#"
                    SELECT
                        r.id, r.topic_id, r.author_id,
                        u.username   AS author_username,
                        u.real_name  AS author_real_name,
                        u.avatar_url AS author_avatar_url,
                        u.role       AS "author_role: _",
                        r.content, r.reply_to_reply_id,
                        r.created_at, r.updated_at
                    FROM community_topic_replies r
                    JOIN users u ON u.id = r.author_id
                    WHERE r.topic_id = $1
                      AND (r.created_at, r.id) > ($2, $3)
                    ORDER BY r.created_at ASC, r.id ASC
                    LIMIT $4
                    "#,
                    topic_id,
                    cursor_created_at,
                    cursor_id,
                    fetch_limit,
                )
                .fetch_all(pool)
                .await?
            }
            _ => {
                sqlx::query_as!(
                    CommunityTopicReplyResponse,
                    r#"
                    SELECT
                        r.id, r.topic_id, r.author_id,
                        u.username   AS author_username,
                        u.real_name  AS author_real_name,
                        u.avatar_url AS author_avatar_url,
                        u.role       AS "author_role: _",
                        r.content, r.reply_to_reply_id,
                        r.created_at, r.updated_at
                    FROM community_topic_replies r
                    JOIN users u ON u.id = r.author_id
                    WHERE r.topic_id = $1
                    ORDER BY r.created_at ASC, r.id ASC
                    LIMIT $2
                    "#,
                    topic_id,
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
            .map(|r| (Some(r.created_at), Some(r.id)))
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

/// 通过 ID 查询单条独立社区话题回复（含作者信息）。
pub async fn find_standalone_topic_reply_by_id(
    pool: &PgPool,
    reply_id: Uuid,
) -> Result<Option<CommunityTopicReplyResponse>, sqlx::Error> {
    sqlx::query_as!(
        CommunityTopicReplyResponse,
        r#"
        SELECT
            r.id, r.topic_id, r.author_id,
            u.username   AS author_username,
            u.real_name  AS author_real_name,
            u.avatar_url AS author_avatar_url,
            u.role       AS "author_role: _",
            r.content, r.reply_to_reply_id,
            r.created_at, r.updated_at
        FROM community_topic_replies r
        JOIN users u ON u.id = r.author_id
        WHERE r.id = $1
        "#,
        reply_id,
    )
    .fetch_optional(pool)
    .await
}

/// 创建独立社区话题回复，同时递增 topic.reply_count 并插入 @提及记录（同一事务）。
pub async fn create_standalone_topic_reply(
    pool: &PgPool,
    topic_id: Uuid,
    author_id: Uuid,
    content: &str,
    reply_to_reply_id: Option<Uuid>,
    mention_user_ids: &[Uuid],
) -> Result<CommunityTopicReplyResponse, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let reply_id: Uuid = sqlx::query_scalar!(
        r#"
        INSERT INTO community_topic_replies
            (id, topic_id, author_id, content, reply_to_reply_id, created_at, updated_at)
        VALUES (gen_random_uuid(), $1, $2, $3, $4, NOW(), NOW())
        RETURNING id
        "#,
        topic_id,
        author_id,
        content,
        reply_to_reply_id,
    )
    .fetch_one(&mut *tx)
    .await?;

    sqlx::query!(
        r#"UPDATE community_topics SET reply_count = reply_count + 1, updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    for &mentioned_id in mention_user_ids {
        sqlx::query!(
            r#"
            INSERT INTO community_topic_mentions (id, reply_id, mentioned_user_id, created_at)
            VALUES (gen_random_uuid(), $1, $2, NOW())
            "#,
            reply_id,
            mentioned_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    find_standalone_topic_reply_by_id(pool, reply_id)
        .await?
        .ok_or(sqlx::Error::RowNotFound)
}

/// 删除独立社区话题回复，同时递减 topic.reply_count（同一事务）。
pub async fn delete_standalone_topic_reply(
    pool: &PgPool,
    reply_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    let maybe_topic_id: Option<Uuid> = sqlx::query_scalar!(
        r#"SELECT topic_id FROM community_topic_replies WHERE id = $1"#,
        reply_id,
    )
    .fetch_optional(&mut *tx)
    .await?;

    let Some(topic_id) = maybe_topic_id else {
        tx.rollback().await?;
        return Ok(false);
    };

    sqlx::query!(
        r#"DELETE FROM community_topic_replies WHERE id = $1"#,
        reply_id,
    )
    .execute(&mut *tx)
    .await?;

    sqlx::query!(
        r#"UPDATE community_topics SET reply_count = GREATEST(0, reply_count - 1), updated_at = NOW() WHERE id = $1"#,
        topic_id,
    )
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(true)
}
