use axum::{http::StatusCode, Json, response::IntoResponse};
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use base64::Engine;
use base64::engine::general_purpose;
use metrics::counter;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sqlx::{Error, PgPool};
use sqlx::error::ErrorKind;
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CountedLinkStatistic {
    pub amount: Option<i64>,
    pub referer: Option<String>,
    pub user_agent: Option<String>,
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
    headers: HeaderMap,
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

    let referer_header = get_header("referer", &headers);
    let user_agent_header = get_header("user-agent", &headers);

    let insert_statistics_timeout = tokio::time::Duration::from_millis(300);
    let saved_statistic = tokio::time::timeout(
        insert_statistics_timeout,
        sqlx::query(
            r#"
            insert into link_statistics (link_id, referer, user_agent)
            values ($1, $2, $3)
            "#
        ).bind(&requested_link).bind(&referer_header).bind(&user_agent_header)
            .execute(&pool),
    ).await;

    match saved_statistic {
        Err(elapsed) => tracing::error!("saving new link click resulted in a timeout: {}", elapsed),
        Ok(Err(err)) => tracing::error!("saving new link click resulted in error: {}", err),
        _ => tracing::debug!(
            "persisted new link click for link with id {}, referer {} and user-agent {}",
            requested_link,
            referer_header.unwrap_or_default(),
            user_agent_header.unwrap_or_default(),
        ),
    };

    Ok(Response::builder()
        .status(StatusCode::TEMPORARY_REDIRECT)
        .header("Location", link.target_url)
        .header("Cache-Control", DEFAULT_CACHE_CONTROL_HEADER_VALUE)
        .body(Body::empty())
        .expect("this response should always be constructable")
    )
}

fn get_header(key: &str, header_map: &HeaderMap) -> Option<String> {
    header_map.get(key)
        .map(|value| value.to_str().unwrap_or_default().to_string())
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

    for _ in 1..=3 {
        let new_link_id = generate_id();

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
            ).fetch_one(&pool),
        ).await.map_err(internal_error)?;
        
        match new_link {
            Ok(link) => {
                tracing::debug!("created new link with id {} targeting {}", new_link_id, url);
                return Ok(Json(link))
            }
            Err(err) => match err {
                Error::Database(db_err) if db_err.kind() == ErrorKind::UniqueViolation => {}
                _ => return Err(internal_error(err))
            }
        }
    }

    tracing::error!("could not persist new short link: exhausted all retries for generating a unique id");
    let counter = counter!("saving_link_impossible_no_unique_id");
    counter.increment(1);

    Err((StatusCode::INTERNAL_SERVER_ERROR, "internal server error".into()))
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
        ).fetch_one(&pool),
    ).await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    tracing::debug!("updated link with id {} to target {}", link_id, url);

    Ok(Json(link))
}

///
/// Route for retrieving link statistics
///
pub async fn get_link_statistics(
    State(pool): State<PgPool>,
    Path(link_id): Path<String>,
) -> Result<Json<Vec<CountedLinkStatistic>>, (StatusCode, String)> {
    let fetch_statistics_timeout = tokio::time::Duration::from_millis(300);
    let statistics = tokio::time::timeout(
        fetch_statistics_timeout,
        sqlx::query_as!(
            CountedLinkStatistic,
            r#"
            select count(*) as amount, referer, user_agent from link_statistics
                group by link_id, referer, user_agent
                having link_id = $1
            "#,
            &link_id,
        ).fetch_all(&pool),
    ).await
        .map_err(internal_error)?
        .map_err(internal_error)?;

    tracing::debug!("statistics for link with id {} requested", link_id);

    Ok(Json(statistics))
}