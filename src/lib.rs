pub mod config;
pub mod error;
pub mod models;
pub mod validation;
pub mod repository;

use axum::{Router, routing::get};
use error::AppError;

// router tek bir yerde kuruyoruz, hem main hem de test dosyaları aynı router kullanabilir.
pub fn build_router() -> Router {
    Router::new()
        .route("/health", get(health))
        .fallback(not_found)
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
