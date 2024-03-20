use axum::{http::StatusCode, Json, response::IntoResponse};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower::ServiceExt;
use url::Url;

use crate::utils::internal_error;

const DEFAULT_CACHE_CONTROL_HEADER_VALUE: &str =
    "public, max-age=300, s-maxage=300, stale-while-revalidate=300, stale-if-error=300";

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Link {
    pub id: String,
    pub target_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTarget {
    pub target_url: String,
}

///
/// Health check route - intended to be called if the service is hosted and monitored.
///
pub async fn health() -> impl IntoResponse {
    (StatusCode::OK, "Service is healthy")
}

///
/// The redirect route
///
pub async fn redirect(
    State(pool): State<PgPool>,
    Path(requested_link): Path<String>,
) -> Result<Response, (StatusCode, String)> {
    let select_timeout = tokio::time::Duration::from_millis(300);
    let link = tokio::time::timeout(
        select_timeout,
        sqlx::query_as!(
            Link,
            "select id, target_url from links where id = $1",
            requested_link,
        ).fetch_optional(&pool),
    )
        .await
        .map_err(internal_error)?
        .map_err(internal_error)?
        .ok_or_else(|| "not found".to_string())
        .map_err(|err| (StatusCode::NOT_FOUND, err))?;

    tracing::debug!(
        "redirecting link id {}_to {}",
        requested_link,
        link.target_url
    );

    Ok(Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header("Location", link.target_url)
        .header("Cache-Control", DEFAULT_CACHE_CONTROL_HEADER_VALUE)
        .body(Body::empty())
        .expect("this response should always be constructable")
    )
}

pub fn generate_id() -> String {
    let random_number = rand::thread_rng().gen_range(0..u32::MAX);
    general_purpose::URL_SAFE_NO_PAD.encode(random_number.to_string())
}

///
/// Route for creating new redirect links
///
pub async fn create_link(
    State(pool): State<PgPool>,
    Json(new_link): Json<LinkTarget>,
) -> Result<Json<Link>, (StatusCode, String)> {
    let url = Url::parse(&new_link.target_url)
        .map_err(|_| (StatusCode::CONFLICT, "bad url".into()))?
        .to_string();

    let new_link_id = generate_id();
    let insert_link_timeout = tokio::time::Duration::from_millis(300);
    let new_link = tokio::time::timeout(
        insert_link_timeout,
        sqlx::query_as!(
            Link,
            r#"
            with inserted_link as (
                insert into links(id, target_url)
                values ($1, $2)
                returning id, target_url
            )
            select id, target_url from inserted_link
            "#,
            &new_link_id,
            &url
        ).fetch_one(&pool)
    ).await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    tracing::debug!("created new link with id {} targetting {}", new_link_id, url);

    Ok(Json(new_link))
}

///
/// Route for updating existing links
///
pub async fn update_link(
    State(pool): State<PgPool>,
    Path(link_id): Path<String>,
    Json(update_link): Json<LinkTarget>,
) -> Result<Json<Link>, (StatusCode, String)> {
    let url = Url::parse(&update_link.target_url)
        .map_err(|_| (StatusCode::CONFLICT, "malformed url".into()))?
        .to_string();

    let update_link_timeout = tokio::time::Duration::from_millis(300);
    let link = tokio::time::timeout(
        update_link_timeout,
        sqlx::query_as!(
            Link,
            r#"
            with updated_link as (
                update links set target_url = $1 where id = $2
                returning id, target_url
            )
            select id, target_url from updated_link
            "#,
            &url,
            &link_id,
        ).fetch_one(&pool)
    ).await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    tracing::debug!("updated link with id {} to target {}", link_id, url);

    Ok(Json(link))
}