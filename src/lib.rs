use axum::{Router, routing::get};

pub mod config;

// router tek bir yerde kuruyoruz, hem main hem de test dosyaları aynı router kullanabilir.
pub fn build_router() -> Router {
    Router::new().route("/health", get(health))
}

pub fn load_env() {
    dotenvy::dotenv().ok();
}

// handlers/health.rs içerisine taşınacak
async fn health() -> &'static str {
    "ok"
}
