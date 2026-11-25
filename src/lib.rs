pub mod config;
pub mod error;
pub mod handlers;
pub mod models;
pub mod repository;
pub mod service;
pub mod state;
pub mod validation;

use axum::{
    Router,
    routing::{get, post},
};
use error::AppError;

use crate::state::AppState;

// router tek bir yerde kuruyoruz, hem main hem de test dosyaları aynı router kullanabilir.
pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/links", post(handlers::links::create_link))
        .route("/{code}", get(handlers::redirect::redirect))
        .fallback(not_found)
        .with_state(state)
}

pub fn load_env() {
    dotenvy::dotenv().ok();
}

// handlers/health.rs içerisine taşınacak
async fn health() -> &'static str {
    "ok"
}

async fn not_found() -> AppError {
    AppError::NotFound("route not found.".into())
}
