use sqlx::PgPool;

use crate::models::discover::{ActiveTeacherResponse, LatestTopicResponse, PopularCourseRow};

// ---------------------------------------------------------------------------
// 热门课程
//
// 评分公式：score = enrollment_count * 2 + vote_count
//   - enrollment_count：该课程的累计选课人数（COUNT DISTINCT user_id）
//   - vote_count：学生"点赞"数，已持久化在 courses.vote_count
//
// 之所以 enrollment_count 权重更大（×2）：
//   选课是学生用时间投票，比一键"点赞"门槛更高，更能反映课程价值。
//
// 仅统计已发布（published）课程。
// ---------------------------------------------------------------------------

pub async fn popular_courses(pool: &PgPool, limit: i64) -> Result<Vec<PopularCourseRow>, sqlx::Error> {
    sqlx::query_as::<_, PopularCourseRow>(
        r#"
        SELECT
            c.id,
            c.title,
            c.description,
            c.cover_image_url,
            c.major_id,
            c.teacher_id,
            u.real_name         AS teacher_name,
            c.status,
            c.created_at,
            c.updated_at,
            c.vote_count,
            COUNT(DISTINCT e.user_id) AS "enrollment_count",
            MAX(m.name)             AS major_name
        FROM courses c
        LEFT JOIN users u ON u.id = c.teacher_id
        LEFT JOIN majors m ON m.id = c.major_id
        LEFT JOIN course_enrollments e ON e.course_id = c.id
        WHERE c.status = 'published'::course_status
        GROUP BY c.id, u.real_name
        ORDER BY (COUNT(DISTINCT e.user_id) * 2 + c.vote_count) DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// 活跃教师
//
// 判定标准（三级排序）：
//   1. total_student_count：该教师所有已发布课程的累计不重复学生数
//      体现教师对学生的覆盖广度与影响力
//   2. recent_activity_count：近 30 天在课程社区（course_topics / course_topic_replies）
//      及独立社区（community_topics / community_topic_replies）中的互动总数
//      体现近期活跃度，避免"沉睡教师"占据榜单
//   3. published_course_count：已发布课程数（兜底排序）
//
// 准入条件：至少有 1 门已发布课程 + 账号处于启用状态。
// ---------------------------------------------------------------------------

pub async fn active_teachers(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<ActiveTeacherResponse>, sqlx::Error> {
    sqlx::query_as!(
        ActiveTeacherResponse,
        r#"
        SELECT
            u.id                            AS "user_id!",
            u.username                      AS "username!",
            u.real_name,
            u.avatar_url,
            COUNT(DISTINCT c.id)            AS "published_course_count!",
            COUNT(DISTINCT e.user_id)       AS "total_student_count!",
            COALESCE((
                SELECT COUNT(*) FROM course_topics ct
                 WHERE ct.author_id = u.id
                   AND ct.created_at > NOW() - INTERVAL '30 days'
            ), 0)
            + COALESCE((
                SELECT COUNT(*) FROM course_topic_replies ctr
                 WHERE ctr.author_id = u.id
                   AND ctr.created_at > NOW() - INTERVAL '30 days'
            ), 0)
            + COALESCE((
                SELECT COUNT(*) FROM community_topics comt
                 WHERE comt.author_id = u.id
                   AND comt.created_at > NOW() - INTERVAL '30 days'
            ), 0)
            + COALESCE((
                SELECT COUNT(*) FROM community_topic_replies comtr
                 WHERE comtr.author_id = u.id
                   AND comtr.created_at > NOW() - INTERVAL '30 days'
            ), 0)                           AS "recent_activity_count!"
        FROM users u
        JOIN courses c
          ON c.teacher_id = u.id
         AND c.status = 'published'::course_status
        LEFT JOIN course_enrollments e ON e.course_id = c.id
        WHERE u.role = 'teacher'::user_role
          AND u.is_active = TRUE
        GROUP BY u.id, u.username, u.real_name, u.avatar_url
        ORDER BY
            COUNT(DISTINCT e.user_id)  DESC,
            (
                COALESCE((SELECT COUNT(*) FROM course_topics ct
                           WHERE ct.author_id = u.id
                             AND ct.created_at > NOW() - INTERVAL '30 days'), 0)
              + COALESCE((SELECT COUNT(*) FROM course_topic_replies ctr
                           WHERE ctr.author_id = u.id
                             AND ctr.created_at > NOW() - INTERVAL '30 days'), 0)
              + COALESCE((SELECT COUNT(*) FROM community_topics comt
                           WHERE comt.author_id = u.id
                             AND comt.created_at > NOW() - INTERVAL '30 days'), 0)
              + COALESCE((SELECT COUNT(*) FROM community_topic_replies comtr
                           WHERE comtr.author_id = u.id
                             AND comtr.created_at > NOW() - INTERVAL '30 days'), 0)
            ) DESC,
            COUNT(DISTINCT c.id) DESC
        LIMIT $1
        "#,
        limit,
    )
    .fetch_all(pool)
    .await
}

// ---------------------------------------------------------------------------
// 最新话题
//
// 将「课程社区话题」（course_topics）与「独立社区话题」（community_topics）
// 合并后，按 created_at DESC 取前 N 条。
//
// source = "course"    → source_id 为所属课程 ID，source_title 为课程标题
// source = "community" → source_id / source_title 均为 NULL
//
// content_preview 截取前 200 个字符，供列表展示使用。
// ---------------------------------------------------------------------------

pub async fn latest_topics(
    pool: &PgPool,
    limit: i64,
) -> Result<Vec<LatestTopicResponse>, sqlx::Error> {
    // 使用动态 query_as 以支持 UNION ALL 子查询；参数绑定同样安全。
    sqlx::query_as::<_, LatestTopicResponse>(
        r#"
        SELECT
            id,
            author_id,
            author_username,
            author_real_name,
            author_avatar_url,
            author_role,
            title,
            content_preview,
            reply_count,
            created_at,
            source,
            source_id,
            source_title
        FROM (
            SELECT
                t.id,
                t.author_id,
                u.username       AS author_username,
                u.real_name      AS author_real_name,
                u.avatar_url     AS author_avatar_url,
                u.role           AS author_role,
                t.title,
                LEFT(t.content, 200)  AS content_preview,
                t.reply_count,
                t.created_at,
                'course'::text   AS source,
                t.course_id      AS source_id,
                c.title          AS source_title
            FROM course_topics t
            JOIN users u  ON u.id  = t.author_id
            JOIN courses c ON c.id = t.course_id

            UNION ALL

            SELECT
                ct.id,
                ct.author_id,
                u.username,
                u.real_name,
                u.avatar_url,
                u.role,
                ct.title,
                LEFT(ct.content, 200),
                ct.reply_count,
                ct.created_at,
                'community'::text,
                NULL::uuid,
                NULL::text
            FROM community_topics ct
            JOIN users u ON u.id = ct.author_id
        ) sub
        ORDER BY created_at DESC
        LIMIT $1
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await
}
