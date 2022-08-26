pub mod account;
pub mod login;
pub mod logout;
pub mod register;

use std::borrow::Cow;

use askama::DynTemplate;
use async_trait::async_trait;
use axum::{
    extract::{FromRequest, RequestParts},
    http::{self, StatusCode},
    response::{IntoResponse, Response},
    BoxError, Form,
};
use serde::de::DeserializeOwned;
use validator::{Validate, ValidationError, ValidationErrors};

// https://github.com/tokio-rs/axum/blob/main/examples/validator/src/main.rs
#[derive(Debug, Clone, Copy, Default)]
pub struct ValidatedForm<T: ToTemplate>(pub T);

pub trait ToTemplate {
    fn to_template(self, errors: ValidationErrors) -> Box<dyn DynTemplate + 'static>;
}

#[async_trait]
impl<F, B> FromRequest<B> for ValidatedForm<F>
where
    F: DeserializeOwned + Validate + ToTemplate,
    B: http_body::Body + Send,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Rejection = Response;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let Form(form) = Form::<F>::from_request(req)
            .await
            .map_err(|err| err.into_response())?;

        if let Err(errors) = form.validate() {
            Err(form_errors(form, errors))
        } else {
            Ok(ValidatedForm(form))
        }
    }
}

pub fn form_error<F: ToTemplate>(form: F, code: &'static str, message: &'static str) -> Response {
    let mut error = ValidationError::new(code);
    error.message = Some(Cow::Borrowed(message));
    let mut errors = ValidationErrors::new();
    errors.add("username", error);

    form_errors(form, errors)
}

pub fn form_errors<F: ToTemplate>(form: F, errors: ValidationErrors) -> Response {
    let template = form.to_template(errors);
    match template.dyn_render() {
        Ok(body) => {
            let headers = [(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static(template.mime_type()),
            )];

            (headers, body).into_response()
        }
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}
