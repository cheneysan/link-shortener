mod routes;

use std::error::Error;

use axum::Router;
use routes::health;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "link_shortener=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let app = Router::new().route("/health", get(health()));

    let listener = tokio::net::TcpListener::bind("0.0.0.0::3000")
        .await
        .expect("failed to bind TcpListener");

    tracing::debug!(
        "listening on {}",
        listener
            .local_addr()
            .expect("failed to convert listener address to a local address")
    );

    axum::serve(listener, app)
        .await
        .expect("failed to start server");

    Ok(())
}
