use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use url_shortener::{
    build_router,
    config::{Config, StorageKind},
    load_env,
    repository::memory::MemoryRepo,
    service::LinkService,
    state::AppState,
};

// #[tokio::main] makrosu, async main fonksiyonunu çalıştıracak, tokio runtime'i bizim yerimize oluşturur.
#[tokio::main]
async fn main() {
    load_env();

    init_tracing();
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("Configuration error: {err}");
        std::process::exit(1);
    });

    if config.storage == StorageKind::JsonFile {
        eprintln!("warning: json storage is not implemented yet, using memory");
    }

    let repo = Arc::new(MemoryRepo::new());
    let service = Arc::new(LinkService::new(
        repo,
        config.base_url.clone(),
        config.short_code_length,
    ));

    let addr = config.socket_addr();
    let state = AppState::new(service, Arc::new(config));
    let app = build_router(state);

    // .env'den okunacak
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind address");

    println!("server listening on {addr}");

    axum::serve(listener, app).await.expect("server error");
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("url_shortener=debug,tower_http=info,axum=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}
