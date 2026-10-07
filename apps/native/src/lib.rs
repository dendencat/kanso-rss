use std::sync::Mutex;
use tauri::Manager;
use tauri_plugin_opener::OpenerExt;

struct Connection {
    base: url::Url,
    token: String,
}
struct NativeState {
    local: Connection,
    remote: Mutex<Option<Connection>>,
    client: reqwest::Client,
}

#[tauri::command]
async fn api_request(
    state: tauri::State<'_, NativeState>,
    method: String,
    path: String,
    body: Option<String>,
) -> Result<serde_json::Value, String> {
    if !path.starts_with("/api/v1/")
        || path.contains("..")
        || path.contains('#')
        || path.contains('\\')
    {
        return Err("Invalid API path".into());
    }
    let method = match method.as_str() {
        "GET" => reqwest::Method::GET,
        "POST" => reqwest::Method::POST,
        "PATCH" => reqwest::Method::PATCH,
        "DELETE" => reqwest::Method::DELETE,
        _ => return Err("Invalid method".into()),
    };
    let (url, token) = {
        let remote = state.remote.lock().map_err(|_| "Connection lock failed")?;
        let connection = remote.as_ref().unwrap_or(&state.local);
        (
            connection.base.join(&path).map_err(|_| "Invalid URL")?,
            connection.token.clone(),
        )
    };
    let mut request = state.client.request(method, url).bearer_auth(token);
    if let Some(body) = body {
        if body.len() > 1024 * 1024 {
            return Err("Request too large".into());
        }
        let content_type = if path.starts_with("/api/v1/opml") {
            "application/xml"
        } else {
            "application/json"
        };
        request = request.header("content-type", content_type).body(body);
    }
    let mut response = request.send().await.map_err(|_| "Cannot reach API")?;
    let status = response.status().as_u16();
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Cannot read API response")?
    {
        if bytes.len() + chunk.len() > 32 * 1024 * 1024 {
            return Err("Response too large".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    let text = String::from_utf8(bytes).map_err(|_| "Invalid API response")?;
    Ok(serde_json::json!({"status":status,"text":text}))
}

#[tauri::command]
fn connect_remote(
    state: tauri::State<'_, NativeState>,
    endpoint: String,
    token: String,
) -> Result<(), String> {
    let base = url::Url::parse(&endpoint).map_err(|_| "Invalid endpoint")?;
    if base.scheme() != "https"
        || !base.username().is_empty()
        || base.password().is_some()
        || base.query().is_some()
        || base.fragment().is_some()
        || base.path() != "/"
    {
        return Err("Use an HTTPS origin without credentials, path, query or fragment".into());
    }
    if !(32..=512).contains(&token.len()) {
        return Err("Invalid token length".into());
    }
    *state.remote.lock().map_err(|_| "Connection lock failed")? = Some(Connection { base, token });
    Ok(())
}
#[tauri::command]
fn disconnect_remote(state: tauri::State<'_, NativeState>) -> Result<(), String> {
    *state.remote.lock().map_err(|_| "Connection lock failed")? = None;
    Ok(())
}
#[tauri::command]
fn open_article(app: tauri::AppHandle, url: String) -> Result<(), String> {
    let url = kanso_core::fetch::safe_article_url(&url).ok_or("Unsafe URL")?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "Cannot open article".into())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let navigation_app = app.handle().clone();
            tauri::WebviewWindowBuilder::new(
                app,
                "main",
                tauri::WebviewUrl::App("index.html".into()),
            )
            .title("Kanso RSS")
            .inner_size(1280.0, 840.0)
            .min_inner_size(360.0, 560.0)
            .on_navigation(move |url| {
                let local = (url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                    || (matches!(url.scheme(), "http" | "https")
                        && url.host_str() == Some("tauri.localhost"));
                if local {
                    return true;
                }
                if let Some(link) = kanso_core::fetch::safe_article_url(url.as_str()) {
                    let _ = navigation_app.opener().open_url(link, None::<&str>);
                }
                false
            })
            .build()?;
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
            }
            let store = kanso_core::store::Store::open(dir.join("kanso.sqlite"))?;
            let token = kanso_server::token();
            let state = kanso_server::AppState::new(store, &token)?;
            let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
            let address = listener.local_addr()?;
            listener.set_nonblocking(true)?;
            tauri::async_runtime::spawn(async move {
                let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                    return;
                };
                let refresh = state.clone();
                tokio::spawn(async move {
                    let mut interval = tokio::time::interval(std::time::Duration::from_secs(900));
                    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        interval.tick().await;
                        let _ = kanso_server::start_refresh(refresh.clone()).await;
                    }
                });
                let _ = axum_serve(listener, state).await;
            });
            app.manage(NativeState {
                local: Connection {
                    base: url::Url::parse(&format!("http://{address}/"))?,
                    token,
                },
                remote: Mutex::new(None),
                client: reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .timeout(std::time::Duration::from_secs(40))
                    .build()?,
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            api_request,
            connect_remote,
            disconnect_remote,
            open_article
        ])
        .run(tauri::generate_context!())
        .expect("Failed to start Kanso RSS");
}

async fn axum_serve(
    listener: tokio::net::TcpListener,
    state: kanso_server::AppState,
) -> Result<(), std::io::Error> {
    kanso_server::serve_local(listener, state).await
}
