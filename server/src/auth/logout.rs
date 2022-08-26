use crate::ServerError;
use axum::{response::Redirect, Extension};
use axum_sessions::async_session::Session;
use sqlx::SqlitePool;

pub async fn get_logout(
    Extension(session): Extension<Session>,
    Extension(pool): Extension<SqlitePool>,
) -> Result<Redirect, ServerError> {
    sqlx::query(
        r#"
            DELETE FROM sessions
            WHERE session_id = $1
        "#,
    )
    .bind(&session.id())
    .fetch_optional(&pool)
    .await?;

    Ok(Redirect::to("/"))
}
