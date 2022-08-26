mod auth;
mod db;
mod error;
mod game;
mod index;

use error::*;

use axum::{
    http::StatusCode,
    routing::{get, get_service, post},
    Extension, Router,
};
use axum_sessions::{async_session::MemoryStore, SessionLayer};
use std::{net::SocketAddr, path::PathBuf};
use tower_http::{
    services::ServeDir,
    trace::{DefaultMakeSpan, TraceLayer},
};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();

    let pool = db::setup().await?;

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG")
                .unwrap_or_else(|_| "example_websockets=debug,tower_http=debug".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let assets_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("public");

    let store = MemoryStore::new();
    let secret = b"7w!z%C*F-JaNdRgUjXn2r5u8x/A?D(G+KbPeShVmYp3s6v9y$B&E)H@McQfTjWnZ";
    let session_layer = SessionLayer::new(store, secret);

    let game_state = game::GameState::new(pool.clone()).await;

    // build our application with some routes
    let app = Router::new()
        .fallback(
            get_service(ServeDir::new(assets_dir).append_index_html_on_directories(true))
                .handle_error(|error: std::io::Error| async move {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("Unhandled internal error: {}", error),
                    )
                }),
        )
        .route("/", get(index::get_index))
        .route("/game", get(game::get_game))
        .route("/game/ws", get(game::ws_handler))
        .route(
            "/register",
            get(auth::register::get_register).post(auth::register::post_register),
        )
        .route(
            "/login",
            get(auth::login::get_login).post(auth::login::post_login),
        )
        .route("/logout", get(auth::logout::get_logout))
        .route("/account", get(auth::account::get_account))
        .route(
            "/account/username",
            post(auth::account::post_change_username),
        )
        .route("/account/email", post(auth::account::post_change_email))
        .route(
            "/account/password",
            post(auth::account::post_change_password),
        )
        .layer(Extension(game_state))
        .layer(Extension(pool.clone()))
        .layer(session_layer)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::default().include_headers(true)),
        );

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();

    Ok(())
}
