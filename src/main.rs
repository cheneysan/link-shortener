use std::error::Error;

use axum::Router;
use axum::routing::get;
use axum_prometheus::PrometheusMetricLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;

use routes::health;

mod routes;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "link_shortener=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let (prometheus_layer, metric_handle) = PrometheusMetricLayer::pair();

    let app = Router::new()
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .layer(prometheus_layer);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
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
