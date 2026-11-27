use std::sync::Arc;

use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use url_shortener::{
    build_router,
    config::{
        Config,
        StorageKind::{self},
    },
    load_env,
    repository::{UrlRepository, json_file::JsonFileRepo, memory::MemoryRepo},
    service::LinkService,
    state::AppState,
};

// #[tokio::main] makrosu, async main fonksiyonunu çalıştıracak, tokio runtime'i bizim yerimize oluşturur.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_env();
    init_tracing();

    let config = Config::from_env()?;
    let repo: Arc<dyn UrlRepository> = match config.storage {
        StorageKind::JsonFile => {
            tracing::info!(file=%config.data_file.display(), "storage: json file");
            Arc::new(JsonFileRepo::open(&config.data_file).await?)
        }
        StorageKind::Memory => {
            tracing::info!("storage: in-memory");
            Arc::new(MemoryRepo::new())
        }
    };

    let service = Arc::new(LinkService::new(
        repo,
        config.base_url.clone(),
        config.short_code_length,
    ));

    let addr = config.socket_addr();
    let state = AppState {
        service,
        config: Arc::new(config.clone()),
    };

    let app = build_router(state);

    // .env'den okunacak
    let listener = tokio::net::TcpListener::bind(addr).await?;
    // % / ? -> display formatında göster, tracing makrosu
    tracing::info!(addr = %config.socket_addr(), "listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("server stopped");
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("url_shortener=debug,tower_http=info,axum=info"));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    // unix tabanlı olmayan işletim sisteminde derlenirse devreye girer
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
            _ = terminate => {}
    }

    tracing::info!("shutdown signal received, draining connetions");
}
