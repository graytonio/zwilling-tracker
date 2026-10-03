use axum::{
    Json, Router,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::Html,
    routing::get,
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

const MAX_CONTENTS_LEN: usize = 2000;

#[tokio::main]
async fn main() {
    let db_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let pool = PgPool::connect(&db_url).await.unwrap();
    migrate(&pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("listening on http://localhost:3000");
    axum::serve(listener, app(pool)).await.unwrap();
}

async fn migrate(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::raw_sql(
        "CREATE TABLE IF NOT EXISTS containers (
            container_code TEXT PRIMARY KEY,
            product_code TEXT NOT NULL,
            size TEXT,
            contents TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
        )",
    )
    .execute(pool)
    .await?;
    Ok(())
}

fn app(pool: PgPool) -> Router {
    Router::new()
        .route("/", get(|| async { Html(include_str!("index.html")) }))
        .route(
            "/jsQR.min.js",
            get(|| async {
                (
                    [(header::CONTENT_TYPE, "text/javascript")],
                    include_str!("jsQR.min.js"),
                )
            }),
        )
        .route("/api/containers", get(search_containers))
        .route(
            "/api/containers/{code}",
            get(get_container)
                .put(put_container)
                .delete(delete_container),
        )
        .with_state(pool)
}

#[derive(Serialize)]
struct Contents {
    contents: Option<String>,
}

#[derive(Deserialize)]
struct Search {
    q: Option<String>,
}

#[derive(Serialize)]
struct Container {
    container: String,
    product: String,
    size: Option<String>,
    contents: String,
}

#[derive(Deserialize)]
struct Update {
    product: String,
    size: Option<String>,
    contents: String,
}

type ApiResult<T> = Result<T, (StatusCode, String)>;

fn internal(e: sqlx::Error) -> (StatusCode, String) {
    eprintln!("db error: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, "database error".into())
}

async fn get_container(
    State(pool): State<PgPool>,
    Path(code): Path<String>,
) -> ApiResult<Json<Contents>> {
    let contents: Option<String> =
        sqlx::query_scalar("SELECT contents FROM containers WHERE container_code = $1")
            .bind(code)
            .fetch_optional(&pool)
            .await
            .map_err(internal)?;
    Ok(Json(Contents { contents }))
}

/// Case-insensitive substring match on contents or container code; an empty query lists everything.
async fn search_containers(
    State(pool): State<PgPool>,
    Query(Search { q }): Query<Search>,
) -> ApiResult<Json<Vec<Container>>> {
    let rows: Vec<(String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT container_code, product_code, size, contents FROM containers
         WHERE strpos(lower(contents), lower($1)) > 0
            OR strpos(lower(container_code), lower($1)) > 0
         ORDER BY updated_at DESC
         LIMIT 100",
    )
    .bind(q.unwrap_or_default().trim())
    .fetch_all(&pool)
    .await
    .map_err(internal)?;
    Ok(Json(
        rows.into_iter()
            .map(|(container, product, size, contents)| Container {
                container,
                product,
                size,
                contents,
            })
            .collect(),
    ))
}

async fn put_container(
    State(pool): State<PgPool>,
    Path(code): Path<String>,
    Json(u): Json<Update>,
) -> ApiResult<StatusCode> {
    if u.contents.chars().count() > MAX_CONTENTS_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("contents must be at most {MAX_CONTENTS_LEN} characters"),
        ));
    }
    sqlx::query(
        "INSERT INTO containers (container_code, product_code, size, contents)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (container_code) DO UPDATE
         SET product_code = $2, size = $3, contents = $4, updated_at = now()",
    )
    .bind(code)
    .bind(u.product)
    .bind(u.size)
    .bind(u.contents)
    .execute(&pool)
    .await
    .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_container(
    State(pool): State<PgPool>,
    Path(code): Path<String>,
) -> ApiResult<StatusCode> {
    sqlx::query("DELETE FROM containers WHERE container_code = $1")
        .bind(code)
        .execute(&pool)
        .await
        .map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

// Needs a running Postgres at DATABASE_URL (`mise run test` starts one).
// Each test uses its own container codes so they can share the database and run in parallel.
#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{Value, json};
    use tower::ServiceExt;

    async fn setup() -> Router {
        let pool =
            PgPool::connect(&std::env::var("DATABASE_URL").expect("DATABASE_URL must be set"))
                .await
                .unwrap();
        migrate(&pool).await.unwrap();
        app(pool)
    }

    async fn call(
        app: &Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let req = Request::builder().method(method).uri(uri);
        let req = match body {
            Some(b) => req
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(b.to_string())),
            None => req.body(Body::empty()),
        }
        .unwrap();
        let res = app.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    fn update(contents: &str) -> Option<Value> {
        Some(json!({ "product": "12LB02", "size": "m", "contents": contents }))
    }

    #[tokio::test]
    async fn save_edit_clear_roundtrip() {
        let app = setup().await;
        let uri = "/api/containers/T_ROUNDTRIP";
        call(&app, "DELETE", uri, None).await;

        assert_eq!(
            call(&app, "GET", uri, None).await,
            (StatusCode::OK, json!({ "contents": null }))
        );
        assert_eq!(
            call(&app, "PUT", uri, update("Chili")).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "GET", uri, None).await.1,
            json!({ "contents": "Chili" })
        );
        assert_eq!(
            call(&app, "PUT", uri, update("Soup")).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "GET", uri, None).await.1,
            json!({ "contents": "Soup" })
        );
        assert_eq!(
            call(&app, "DELETE", uri, None).await.0,
            StatusCode::NO_CONTENT
        );
        assert_eq!(
            call(&app, "GET", uri, None).await.1,
            json!({ "contents": null })
        );
    }

    #[tokio::test]
    async fn rejects_bad_updates() {
        let app = setup().await;
        let uri = "/api/containers/T_INVALID";
        let too_long = "a".repeat(MAX_CONTENTS_LEN + 1);
        assert_eq!(
            call(&app, "PUT", uri, update(&too_long)).await.0,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            call(&app, "PUT", uri, Some(json!({}))).await.0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            call(&app, "GET", uri, None).await.1,
            json!({ "contents": null })
        );
    }

    #[tokio::test]
    async fn search_matches_contents_and_code() {
        let app = setup().await;
        call(
            &app,
            "PUT",
            "/api/containers/T_SEARCH_A",
            update("Beef CHILI xq7"),
        )
        .await;
        call(
            &app,
            "PUT",
            "/api/containers/T_SEARCH_B",
            update("Chicken soup xq7 100%"),
        )
        .await;

        let codes = |v: Value| -> Vec<String> {
            let mut c: Vec<String> = v
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["container"].as_str().unwrap().to_string())
                .filter(|c| c.starts_with("T_SEARCH"))
                .collect();
            c.sort();
            c
        };
        let search = |q: &str| format!("/api/containers?q={q}");

        assert_eq!(
            codes(call(&app, "GET", &search("xq7"), None).await.1),
            ["T_SEARCH_A", "T_SEARCH_B"]
        );
        assert_eq!(
            codes(call(&app, "GET", &search("chili%20xq7"), None).await.1),
            ["T_SEARCH_A"]
        );
        assert_eq!(
            codes(call(&app, "GET", &search("t_search_b"), None).await.1),
            ["T_SEARCH_B"]
        );
        // LIKE wildcards are matched literally.
        assert_eq!(
            codes(call(&app, "GET", &search("xq7%20100%25"), None).await.1),
            ["T_SEARCH_B"]
        );
        assert_eq!(
            codes(call(&app, "GET", &search("x_7"), None).await.1),
            Vec::<String>::new()
        );

        call(&app, "DELETE", "/api/containers/T_SEARCH_A", None).await;
        call(&app, "DELETE", "/api/containers/T_SEARCH_B", None).await;
    }
}
