use axum::http::StatusCode;
use metrics::counter;

pub fn internal_error<E>(err: E) -> (StatusCode, String)
    where
        E: std::error::Error,
{
    tracing::error!("{}", err);
    let counter = counter!("request_error", "error" => format!("{}!", err));
    counter.increment(1);

    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}