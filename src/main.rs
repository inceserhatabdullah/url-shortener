use url_shortener::{build_router, config::Config, load_env};

// #[tokio::main] makrosu, async main fonksiyonunu çalıştıracak, tokio runtime'i bizim yerimize oluşturur.
#[tokio::main]
async fn main() {
    load_env();

    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("Configuration error: {err}");
        std::process::exit(1);
    });


    let app = build_router();
    let addr = config.socket_addr();

    // .env'den okunacak
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("failed to bind address");

    println!("server listening on {addr}");

    axum::serve(listener, app).await.expect("server error");
}
