use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{header, HeaderValue, Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use kanso_core::{store::Store, ArticlePatch, ArticleQuery, NewSubscription};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use subtle::ConstantTimeEq;
use tokio::sync::Semaphore;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    token_hash: [u8; 32],
    requests: Arc<Semaphore>,
    refresh: Arc<Semaphore>,
    progress: Arc<Mutex<RefreshProgress>>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RefreshProgress {
    pub running: bool,
    pub processed: usize,
    pub total: usize,
    pub added: usize,
    pub errors: usize,
}

impl AppState {
    pub fn new(store: Store, token: &str) -> anyhow::Result<Self> {
        if token.len() < 32
            || token.len() > 512
            || !token
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            anyhow::bail!("Token must be 32–512 ASCII letters, digits, '-' or '_'");
        }
        Ok(Self {
            store: Arc::new(Mutex::new(store)),
            token_hash: Sha256::digest(token.as_bytes()).into(),
            requests: Arc::new(Semaphore::new(32)),
            refresh: Arc::new(Semaphore::new(1)),
            progress: Arc::new(Mutex::new(RefreshProgress::default())),
        })
    }

    async fn db<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Store) -> anyhow::Result<T> + Send + 'static,
    ) -> Result<T, ApiError> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = store
                .lock()
                .map_err(|_| anyhow::anyhow!("Database lock failed"))?;
            operation(&mut guard)
        })
        .await
        .map_err(|_| ApiError::internal())?
        .map_err(|error| {
            tracing::warn!(error = %error, "Database operation failed");
            ApiError(
                StatusCode::BAD_REQUEST,
                "Operation failed; check input and capacity limits".into(),
            )
        })
    }
}

pub fn token() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::rng().random();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub async fn serve_local(
    listener: tokio::net::TcpListener,
    state: AppState,
) -> Result<(), std::io::Error> {
    axum::serve(listener, router(state)).await
}

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        .route("/stats", get(stats))
        .route("/feeds", get(feeds).post(add_feed))
        .route(
            "/feeds/{id}",
            axum::routing::patch(update_feed).delete(delete_feed),
        )
        .route("/feeds/{id}/refresh", post(refresh_feed))
        .route("/articles", get(articles))
        .route("/articles/{id}", get(article).patch(patch_article))
        .route("/mark-read", post(mark_read))
        .route("/refresh", post(refresh_all).get(refresh_status))
        .route("/opml", get(export_opml).post(import_opml))
        .route_layer(middleware::from_fn_with_state(state.clone(), authenticate));
    Router::new()
        .nest("/api/v1", api)
        .route("/healthz", get(|| async { "ok" }))
        .route("/", get(|| async { static_file("index.html") }))
        .route("/{file}", get(asset))
        .layer(DefaultBodyLimit::max(kanso_core::opml::MAX_OPML_BYTES))
        .layer(middleware::from_fn(security_headers))
        .with_state(state)
}

async fn authenticate(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let candidate = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .unwrap_or("");
    let candidate_hash: [u8; 32] = Sha256::digest(candidate.as_bytes()).into();
    if candidate.len() > 512 || !bool::from(state.token_hash.ct_eq(&candidate_hash)) {
        return ApiError(StatusCode::UNAUTHORIZED, "Authentication required".into())
            .into_response();
    }
    let Ok(_permit) = state.requests.clone().try_acquire_owned() else {
        return ApiError(
            StatusCode::TOO_MANY_REQUESTS,
            "Too many concurrent requests".into(),
        )
        .into_response();
    };
    next.run(request).await
}

async fn security_headers(request: Request<Body>, next: Next) -> Response {
    let mut response = next.run(request).await;
    for (name,value) in [
        ("content-security-policy","default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src 'self'; manifest-src 'self'; worker-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'"),
        ("x-content-type-options","nosniff"),("referrer-policy","no-referrer"),
        ("x-frame-options","DENY"),("permissions-policy","camera=(), microphone=(), geolocation=()"),
        ("cache-control","no-store"),
    ] { response.headers_mut().insert(name,HeaderValue::from_static(value)); }
    response
}

struct ApiError(StatusCode, String);
impl ApiError {
    fn internal() -> Self {
        Self(StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
    }
    fn not_found() -> Self {
        Self(StatusCode::NOT_FOUND, "Not found".into())
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({"error":self.1}))).into_response()
    }
}
type ApiResult<T> = Result<Json<T>, ApiError>;

async fn stats(State(s): State<AppState>) -> ApiResult<kanso_core::Stats> {
    Ok(Json(s.db(|db| db.stats()).await?))
}
async fn feeds(State(s): State<AppState>) -> ApiResult<Vec<kanso_core::Subscription>> {
    Ok(Json(s.db(|db| db.feeds()).await?))
}
async fn add_feed(
    State(s): State<AppState>,
    Json(value): Json<NewSubscription>,
) -> Result<impl IntoResponse, ApiError> {
    let f = s.db(move |db| db.add_feed(&value)).await?;
    Ok((StatusCode::CREATED, Json(f)))
}
#[derive(Deserialize)]
struct FeedPatch {
    title: String,
    folder: String,
}
async fn update_feed(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<FeedPatch>,
) -> Result<StatusCode, ApiError> {
    if s.db(move |db| db.update_feed(&id, &patch.title, &patch.folder))
        .await?
    {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}
async fn delete_feed(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if s.db(move |db| db.delete_feed(&id)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}
async fn articles(
    State(s): State<AppState>,
    Query(q): Query<ArticleQuery>,
) -> ApiResult<Vec<kanso_core::Article>> {
    Ok(Json(s.db(move |db| db.articles(&q)).await?))
}
async fn article(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<kanso_core::Article> {
    Ok(Json(
        s.db(move |db| db.article(&id))
            .await?
            .ok_or_else(ApiError::not_found)?,
    ))
}
async fn patch_article(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(patch): Json<ArticlePatch>,
) -> Result<StatusCode, ApiError> {
    if s.db(move |db| db.patch_article(&id, &patch)).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(ApiError::not_found())
    }
}
#[derive(Deserialize)]
struct MarkRead {
    feed_id: Option<String>,
    folder: Option<String>,
}
async fn mark_read(
    State(s): State<AppState>,
    Json(value): Json<MarkRead>,
) -> ApiResult<serde_json::Value> {
    let changed = s
        .db(move |db| db.mark_read(value.feed_id.as_deref(), value.folder.as_deref()))
        .await?;
    Ok(Json(serde_json::json!({"changed":changed})))
}

async fn refresh_one(s: &AppState, feed: kanso_core::Subscription) -> anyhow::Result<usize> {
    match kanso_core::fetch::fetch(&feed.url).await {
        Ok(fetched) => {
            let id = feed.id.clone();
            match s.db(move |db| db.ingest(&id, fetched)).await {
                Ok(n) => Ok(n),
                Err(_) => {
                    s.db(move |db| db.fetch_error(&feed.id, "記事を保存できませんでした。保存上限とディスク容量を確認してください。")).await.ok();
                    Err(anyhow::anyhow!("Could not save feed"))
                }
            }
        }
        Err(error) => {
            tracing::warn!(feed_id=%feed.id,"Feed refresh failed (network or parser); URL omitted to protect feed credentials");
            s.db(move |db| {
                db.fetch_error(
                    &feed.id,
                    "取得に失敗しました。URL、公開ネットワーク、RSS形式を確認してください。",
                )
            })
            .await
            .ok();
            Err(error)
        }
    }
}
async fn refresh_feed(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<serde_json::Value> {
    let _permit = s
        .refresh
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiError(StatusCode::CONFLICT, "Refresh already running".into()))?;
    let feed = s
        .db(move |db| db.feed(&id))
        .await?
        .ok_or_else(ApiError::not_found)?;
    let added = refresh_one(&s, feed)
        .await
        .map_err(|_| ApiError(StatusCode::BAD_GATEWAY, "Feed fetch failed".into()))?;
    Ok(Json(serde_json::json!({"added":added})))
}

pub async fn start_refresh(s: AppState) -> Result<bool, anyhow::Error> {
    let Ok(permit) = s.refresh.clone().try_acquire_owned() else {
        return Ok(false);
    };
    let feeds = s
        .db(|db| db.feeds())
        .await
        .map_err(|_| anyhow::anyhow!("Cannot list feeds"))?;
    *s.progress
        .lock()
        .map_err(|_| anyhow::anyhow!("Lock failed"))? = RefreshProgress {
        running: true,
        total: feeds.len(),
        ..Default::default()
    };
    tokio::spawn(async move {
        let _permit = permit;
        for feed in feeds {
            let result = refresh_one(&s, feed).await;
            if let Ok(mut p) = s.progress.lock() {
                p.processed += 1;
                match result {
                    Ok(n) => p.added += n,
                    Err(_) => p.errors += 1,
                }
            }
        }
        if let Ok(mut p) = s.progress.lock() {
            p.running = false;
        }
    });
    Ok(true)
}
async fn refresh_all(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    if !start_refresh(s).await.map_err(|_| ApiError::internal())? {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "Refresh already running".into(),
        ));
    }
    Ok((
        StatusCode::ACCEPTED,
        Json(serde_json::json!({"started":true})),
    ))
}
async fn refresh_status(State(s): State<AppState>) -> ApiResult<RefreshProgress> {
    Ok(Json(
        s.progress.lock().map_err(|_| ApiError::internal())?.clone(),
    ))
}
async fn export_opml(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let xml = s
        .db(|db| Ok(kanso_core::opml::export(&db.feeds()?)))
        .await?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/xml; charset=utf-8"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=\"kanso.opml\"",
            ),
        ],
        xml,
    ))
}
async fn import_opml(State(s): State<AppState>, body: String) -> ApiResult<serde_json::Value> {
    let imported = s
        .db(move |db| db.import(&kanso_core::opml::parse(&body)?))
        .await?;
    Ok(Json(serde_json::json!({"imported":imported})))
}

async fn asset(Path(file): Path<String>) -> Response {
    static_file(&file)
}
fn static_file(file: &str) -> Response {
    let (mime, body): (&str, &[u8]) = match file {
        "index.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../../../web/index.html"),
        ),
        "app.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../../web/app.js"),
        ),
        "style.css" => (
            "text/css; charset=utf-8",
            include_bytes!("../../../web/style.css"),
        ),
        "manifest.webmanifest" => (
            "application/manifest+json",
            include_bytes!("../../../web/manifest.webmanifest"),
        ),
        "sw.js" => (
            "text/javascript; charset=utf-8",
            include_bytes!("../../../web/sw.js"),
        ),
        "icon.svg" => ("image/svg+xml", include_bytes!("../../../web/icon.svg")),
        "licenses.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../../../web/licenses.html"),
        ),
        "third-party.html" => (
            "text/html; charset=utf-8",
            include_bytes!("../../../web/third-party.html"),
        ),
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    ([(header::CONTENT_TYPE, mime)], body).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt;
    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    fn app() -> Router {
        router(AppState::new(Store::memory().unwrap(), TOKEN).unwrap())
    }
    #[tokio::test]
    async fn authentication_headers_and_validation() {
        let response = app()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/feeds")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        let request = Request::builder()
            .uri("/api/v1/feeds")
            .method("POST")
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(r#"{"url":"http://169.254.169.254/"}"#))
            .unwrap();
        assert_eq!(
            app().oneshot(request).await.unwrap().status(),
            StatusCode::BAD_REQUEST
        );
    }
    #[tokio::test]
    async fn api_roundtrip_and_no_cors() {
        let app = app();
        let request = Request::builder()
            .uri("/api/v1/feeds")
            .method("POST")
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/json")
            .body(Body::from(
                r#"{"url":"https://example.org/rss","title":"Test","folder":"Tech"}"#,
            ))
            .unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert!(response
            .headers()
            .get("access-control-allow-origin")
            .is_none());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/feeds")
                    .header("authorization", format!("Bearer {TOKEN}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 4096)
            .await
            .unwrap();
        let values: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(values[0]["title"], "Test");
    }
    #[tokio::test]
    async fn oversized_import_and_weak_tokens_are_rejected() {
        assert!(AppState::new(Store::memory().unwrap(), "weak").is_err());
        let request = Request::builder()
            .uri("/api/v1/opml")
            .method("POST")
            .header("authorization", format!("Bearer {TOKEN}"))
            .header("content-type", "application/xml")
            .body(Body::from("x".repeat(kanso_core::opml::MAX_OPML_BYTES + 1)))
            .unwrap();
        assert_eq!(
            app().oneshot(request).await.unwrap().status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
    }
}
