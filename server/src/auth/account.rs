use crate::ServerError;
use askama::{DynTemplate, Template};
use askama_axum::Response;
use axum::{
    response::{IntoResponse, Redirect},
    Extension,
};
use axum_sessions::async_session::Session;
use bcrypt::hash;
use serde::Deserialize;
use sqlx::SqlitePool;
use validator::{Validate, ValidationErrors};

use super::{form_error, ToTemplate, ValidatedForm};

#[derive(Debug, Deserialize, Validate)]
pub struct ChangeUsernameForm {
    #[validate(length(min = 1, message = "The username must not be empty"))]
    username: String,
    email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ChangeEmailForm {
    username: String,
    #[validate(email(message = "The email address must be valid"))]
    email: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct ChangePasswordForm {
    username: String,
    email: String,
    #[validate(length(min = 4, message = "Password must contain at least 4 characters"))]
    password: String,
    #[validate(must_match(other = "password", message = "The passwords must match"))]
    password_repeat: String,
}

#[derive(Debug, Deserialize, Validate)]
pub struct Account {
    #[validate(length(min = 1, message = "The username must not be empty"))]
    username: String,
    #[validate(email(message = "The email address must be valid"))]
    email: String,
    #[validate(length(min = 4, message = "Password must contain at least 4 characters"))]
    password: String,
    #[validate(must_match(other = "password", message = "The passwords must match"))]
    password_repeat: String,
}

impl ToTemplate for ChangeUsernameForm {
    fn to_template(self, errors: ValidationErrors) -> Box<dyn DynTemplate> {
        Box::new(AccountTemplate {
            username: self.username,
            username_error: errors
                .field_errors()
                .get("username")
                .unwrap_or(&&Vec::new())
                .iter()
                .filter_map(|error| error.message.as_ref().map(|msg| msg.to_string()))
                .collect(),
            email: self.email,
            email_error: Vec::new(),
            password_error: Vec::new(),
            password_repeat_error: Vec::new(),
        })
    }
}

impl ToTemplate for ChangeEmailForm {
    fn to_template(self, errors: ValidationErrors) -> Box<dyn DynTemplate> {
        Box::new(AccountTemplate {
            username: self.username,
            username_error: Vec::new(),
            email: self.email,
            email_error: errors
                .field_errors()
                .get("email")
                .unwrap_or(&&Vec::new())
                .iter()
                .filter_map(|error| error.message.as_ref().map(|msg| msg.to_string()))
                .collect(),
            password_error: Vec::new(),
            password_repeat_error: Vec::new(),
        })
    }
}

impl ToTemplate for ChangePasswordForm {
    fn to_template(self, errors: ValidationErrors) -> Box<dyn DynTemplate> {
        Box::new(AccountTemplate {
            username: self.username,
            username_error: Vec::new(),
            email: self.email,
            email_error: Vec::new(),
            password_error: errors
                .field_errors()
                .get("password")
                .unwrap_or(&&Vec::new())
                .iter()
                .filter_map(|error| error.message.as_ref().map(|msg| msg.to_string()))
                .collect(),
            password_repeat_error: errors
                .field_errors()
                .get("password_repeat")
                .unwrap_or(&&Vec::new())
                .iter()
                .filter_map(|error| error.message.as_ref().map(|msg| msg.to_string()))
                .collect(),
        })
    }
}

#[derive(Template, Default)]
#[template(path = "account.html")]
pub struct AccountTemplate {
    username: String,
    username_error: Vec<String>,
    email: String,
    email_error: Vec<String>,
    password_error: Vec<String>,
    password_repeat_error: Vec<String>,
}

pub async fn get_account(
    Extension(session): Extension<Session>,
    Extension(pool): Extension<SqlitePool>,
) -> Result<Response, ServerError> {
    let result: Option<(String, String)> = sqlx::query_as(
        r#"
            SELECT username, email
            FROM users
            NATURAL JOIN sessions
            WHERE session_id = $1
        "#,
    )
    .bind(&session.id())
    .fetch_optional(&pool)
    .await?;

    if let Some((username, email)) = result {
        Ok(AccountTemplate {
            username,
            email,
            ..AccountTemplate::default()
        }
        .into_response())
    } else {
        Ok(Redirect::to("/login").into_response())
    }
}

pub async fn post_change_username(
    ValidatedForm(change_username): ValidatedForm<ChangeUsernameForm>,
    Extension(session): Extension<Session>,
    Extension(pool): Extension<SqlitePool>,
) -> Result<Response, ServerError> {
    let result = sqlx::query(
        r#"
            UPDATE users
            SET username = $1
            WHERE user_id = (SELECT user_id
                FROM sessions
                WHERE session_id = $2)
        "#,
    )
    .bind(&change_username.username)
    .bind(&session.id())
    .execute(&pool)
    .await;

    match result {
        Err(_) => Ok(form_error(
            change_username,
            "unique",
            "This username is already taken",
        )),
        Ok(_) => Ok(Redirect::to("/account").into_response()),
    }
}

pub async fn post_change_email(
    ValidatedForm(change_email): ValidatedForm<ChangeEmailForm>,
    Extension(session): Extension<Session>,
    Extension(pool): Extension<SqlitePool>,
) -> Result<Response, ServerError> {
    sqlx::query(
        r#"
            UPDATE users
            SET email = $1
            WHERE user_id = (SELECT user_id
                FROM sessions
                WHERE session_id = $2)
        "#,
    )
    .bind(&change_email.email)
    .bind(&session.id())
    .execute(&pool)
    .await?;

    Ok(Redirect::to("/account").into_response())
}

pub async fn post_change_password(
    ValidatedForm(change_password): ValidatedForm<ChangePasswordForm>,
    Extension(session): Extension<Session>,
    Extension(pool): Extension<SqlitePool>,
) -> Result<Response, ServerError> {
    let password = change_password.password.clone();
    let hashed = tokio::task::spawn_blocking(move || hash(&password, 4).unwrap())
        .await
        .unwrap();

    sqlx::query(
        r#"
            UPDATE users
            SET password = $1
            WHERE user_id = (SELECT user_id
                FROM sessions
                WHERE session_id = $2)
        "#,
    )
    .bind(&hashed)
    .bind(&session.id())
    .execute(&pool)
    .await?;

    Ok(Redirect::to("/account").into_response())
}
