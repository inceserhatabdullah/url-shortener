pub mod config;
pub mod error;
pub mod handlers;
pub mod middleware;
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
    let timeout = std::time::Duration::from_secs(state.config.request_timeout_secs);

    let router = Router::new()
        .route("/health", get(handlers::health::health))
        .route(
            "/api/links",
            post(handlers::links::create_link).get(handlers::links::list_links),
        )
        .route(
            "/api/links/{code}",
            get(handlers::links::get_link).delete(handlers::links::delete_link),
        )
        .route("/{code}", get(handlers::redirect::redirect))
        .fallback(not_found)
        .with_state(state);

    middleware::apply_middleware(router, timeout)
}

pub fn load_env() {
    dotenvy::dotenv().ok();
}

async fn not_found() -> AppError {
    AppError::NotFound("route not found.".into())
}
