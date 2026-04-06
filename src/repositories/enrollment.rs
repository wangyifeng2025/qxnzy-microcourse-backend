use sqlx::PgPool;
use uuid::Uuid;

use crate::models::course::{Course, CourseEnrollment};
use crate::models::pagination::{PageQuery, PagedList};

/// 查询用户是否已报名某课程
pub async fn find_by_user_and_course(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<Option<CourseEnrollment>, sqlx::Error> {
    sqlx::query_as!(
        CourseEnrollment,
        r#"
        SELECT id, user_id, course_id, enrolled_at
        FROM course_enrollments
        WHERE user_id = $1 AND course_id = $2
        "#,
        user_id,
        course_id
    )
    .fetch_optional(pool)
    .await
}

/// 创建选课记录（若已存在则返回现有记录，幂等）
pub async fn enroll(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<CourseEnrollment, sqlx::Error> {
    sqlx::query_as!(
        CourseEnrollment,
        r#"
        INSERT INTO course_enrollments (id, user_id, course_id, enrolled_at)
        VALUES (gen_random_uuid(), $1, $2, NOW())
        ON CONFLICT (user_id, course_id) DO UPDATE
            SET enrolled_at = course_enrollments.enrolled_at
        RETURNING id, user_id, course_id, enrolled_at
        "#,
        user_id,
        course_id
    )
    .fetch_one(pool)
    .await
}

/// 查询用户已选的全部课程（游标分页，按 enrolled_at DESC 排序）
/// 复用 PageQuery：cursor_created_at 对应 enrolled_at，cursor_id 对应 course_enrollments.id
pub async fn find_enrolled_courses_by_user(
    pool: &PgPool,
    user_id: Uuid,
    query: &PageQuery,
) -> Result<PagedList<Course>, sqlx::Error> {
    let page_size = query.page_size();
    let fetch_limit = page_size + 1;

    let mut items = match (query.cursor_created_at, query.cursor_id) {
        (Some(cursor_enrolled_at), Some(cursor_id)) => {
            sqlx::query_as!(
                Course,
                r#"
                SELECT c.id, c.title, c.description, c.cover_image_url, c.major_id,
                       c.teacher_id, u.real_name AS teacher_name,
                       c.status AS "status: _", c.created_at, c.updated_at,
                       c.vote_count
                FROM course_enrollments e
                JOIN courses c ON c.id = e.course_id
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE e.user_id = $1
                  AND (e.enrolled_at, e.id) < ($2, $3)
                ORDER BY e.enrolled_at DESC, e.id DESC
                LIMIT $4
                "#,
                user_id,
                cursor_enrolled_at,
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
                FROM course_enrollments e
                JOIN courses c ON c.id = e.course_id
                LEFT JOIN users u ON u.id = c.teacher_id
                WHERE e.user_id = $1
                ORDER BY e.enrolled_at DESC, e.id DESC
                LIMIT $2
                "#,
                user_id,
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

    // 游标以最后一条的 enrolled_at 为基准；通过子查询取回 enrolled_at
    // 此处用 course.created_at 作为游标占位已不准确，改用 enrollment.enrolled_at
    // 由于 PagedList 游标字段复用 next_cursor_created_at，语义上对应 enrolled_at
    let (next_cursor_created_at, next_cursor_id) = if has_more {
        if let Some(last_course) = items.last() {
            let last_id = last_course.id;
            let enrolled_at: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar!(
                r#"SELECT enrolled_at FROM course_enrollments WHERE user_id = $1 AND course_id = $2"#,
                user_id,
                last_id,
            )
            .fetch_optional(pool)
            .await?;
            (enrolled_at, Some(last_id))
        } else {
            (None, None)
        }
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

/// 统计某门课程的选课总人数
pub async fn count_by_course(pool: &PgPool, course_id: Uuid) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!" FROM course_enrollments WHERE course_id = $1"#,
        course_id
    )
    .fetch_one(pool)
    .await
}

/// 删除选课记录，返回是否实际删除了记录
pub async fn unenroll(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
        DELETE FROM course_enrollments
        WHERE user_id = $1 AND course_id = $2
        "#,
        user_id,
        course_id
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected() > 0)
}

/// 取消选课，并在同一事务中级联清理投票记录（同步维护 vote_count）
/// 返回是否实际删除了选课记录
pub async fn unenroll_with_vote_cascade(
    pool: &PgPool,
    user_id: Uuid,
    course_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let mut tx = pool.begin().await?;

    // 先清理投票记录，若有则同步减票
    let vote_deleted = sqlx::query!(
        r#"DELETE FROM course_votes WHERE user_id = $1 AND course_id = $2"#,
        user_id,
        course_id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    if vote_deleted > 0 {
        sqlx::query!(
            r#"UPDATE courses SET vote_count = GREATEST(0, vote_count - 1) WHERE id = $1"#,
            course_id,
        )
        .execute(&mut *tx)
        .await?;
    }

    // 再删除选课记录
    let enrollment_deleted = sqlx::query!(
        r#"DELETE FROM course_enrollments WHERE user_id = $1 AND course_id = $2"#,
        user_id,
        course_id,
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();

    tx.commit().await?;
    Ok(enrollment_deleted > 0)
}
