use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::IntoResponse;
use metrics::counter;
use sha3::{Digest, Sha3_256};
use sqlx::PgPool;

use crate::utils::internal_error;

struct Settings {
    id: String,
    encrypted_global_api_key: String,
}

pub async fn auth(
    State(pool): State<PgPool>,
    request: Request,
    next: Next,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let api_key = request.headers().get("x-api-key")
        .map(|value| value.to_str().unwrap_or_default())
        .ok_or_else(|| {
            tracing::error!("unauthorized call to API: no API key header received");
            let counter = counter!("unauthorized_calls_count", "uri" => format!("{}!", request.uri()));
            counter.increment(1);

            (StatusCode::UNAUTHORIZED, "unauthorized".into())
        })?;

    let settings = tokio::time::timeout(
        tokio::time::Duration::from_millis(300),
        sqlx::query_as!(
            Settings,
            "select id, encrypted_global_api_key from settings where id = $1",
            "DEFAULT_SETTINGS",
        ).fetch_one(&pool),
    ).await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    let mut hasher = Sha3_256::new();
    hasher.update(api_key.as_bytes());
    let provided_api_key = hasher.finalize();

    if settings.encrypted_global_api_key != format!("{provided_api_key:x}") {
        tracing::error!("unauthenticated call to API: incorrect API key supplied");
        let counter = counter!("unauthorized_calls_count", "uri" => format!("{}!", request.uri()));
        counter.increment(1);

        return Err((StatusCode::UNAUTHORIZED, "unauthorized".into()))
    }

    Ok(next.run(request).await)
}