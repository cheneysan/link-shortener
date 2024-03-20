# Link Shortener - like TinyURL

## Technologies used
* Rust
* Axum (and Tokio behind the scenes)
* sqlx (for talking to Postgres)
* Prometheus (for metrics)
* Open Telemetry (for tracing and logging)

## How to build
sqlx does some syntax magic during compilation to ensure that your SQL scripts are correct. Because of this you will need a DB up and running which is provided for by the `docker-compose.yaml` file.

After that it's the usual `cargo run` for local development.

## DB stuff with sqlx
### Create a database
`sqlx database create`

### Generate migration scripts
`sqlx migrate add -r links`
Results in:
```
Creating migrations\20240320220510_links.up.sql
Creating migrations\20240320220510_links.down.sql
```
These are ready to edit.