use sqlx::PgPool;
use uuid::Uuid;

use crate::models::course::{Chapter, CreateChapterRequest, UpdateChapter};

pub async fn find_by_course_id(pool: &PgPool, course_id: Uuid) -> Result<Vec<Chapter>, sqlx::Error> {
    sqlx::query_as!(
        Chapter,
        r#"
        SELECT id, course_id, title, description, sort_order, created_at, updated_at
        FROM chapters
        WHERE course_id = $1
        ORDER BY sort_order ASC, created_at ASC
        "#,
        course_id
    )
    .fetch_all(pool)
    .await
}

pub async fn find_by_id(pool: &PgPool, id: Uuid) -> Result<Option<Chapter>, sqlx::Error> {
    sqlx::query_as!(
        Chapter,
        r#"
        SELECT id, course_id, title, description, sort_order, created_at, updated_at
        FROM chapters
        WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await
}

pub async fn create(
    pool: &PgPool,
    course_id: Uuid,
    payload: &CreateChapterRequest,
) -> Result<Chapter, sqlx::Error> {
    sqlx::query_as!(
        Chapter,
        r#"
        INSERT INTO chapters (course_id, title, description, sort_order)
        VALUES ($1, $2, $3, COALESCE($4, 0))
        RETURNING id, course_id, title, description, sort_order, created_at, updated_at
        "#,
        course_id,
        payload.title,
        payload.description,
        payload.sort_order,
    )
    .fetch_one(pool)
    .await
}

pub async fn update(
    pool: &PgPool,
    id: Uuid,
    payload: &UpdateChapter,
) -> Result<Option<Chapter>, sqlx::Error> {
    sqlx::query_as!(
        Chapter,
        r#"
        UPDATE chapters
        SET
            title       = COALESCE($2, title),
            description = COALESCE($3, description),
            sort_order  = COALESCE($4, sort_order),
            updated_at  = NOW()
        WHERE id = $1
        RETURNING id, course_id, title, description, sort_order, created_at, updated_at
        "#,
        id,
        payload.title,
        payload.description,
        payload.sort_order,
    )
    .fetch_optional(pool)
    .await
}

pub async fn delete(pool: &PgPool, id: Uuid) -> Result<u64, sqlx::Error> {
    sqlx::query!("DELETE FROM chapters WHERE id = $1", id)
        .execute(pool)
        .await
        .map(|r| r.rows_affected())
}
