use crate::render::{render_from_files, validate_data};
use crate::watcher;
use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use futures_util::{SinkExt, StreamExt};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use tokio::sync::broadcast;
use van_context::project::VanProject;

const PLAYGROUND_HTML: &str = include_str!("playground.html");

#[derive(Clone)]
struct AppState {
    project: VanProject,
    reload_tx: broadcast::Sender<()>,
}

pub async fn run(port: u16) -> Result<()> {
    let project = VanProject::load_cwd().context(
        "Failed to load project. Are you in a Van project?\n\
         Run `van init <name>` to create a new project.",
    )?;

    let (reload_tx, _) = broadcast::channel::<()>(16);
    let version = Arc::new(AtomicU64::new(0));

    // Start file watcher — must keep the watcher alive
    let _watcher = watcher::start(&project.root, version, reload_tx.clone())
        .context("Failed to start file watcher")?;

    let state = AppState {
        project,
        reload_tx,
    };

    let app = Router::new()
        .route("/__van/ws", get(ws_handler))
        .route("/__van/playground", get(playground_handler))
        .route("/__van/playground/{file}", get(playground_file_handler))
        .route("/", get(index_handler))
        .route("/{page}", get(page_handler))
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Failed to bind to {addr}"))?;

    eprintln!("  Van dev server running at http://localhost:{port}");
    eprintln!("  Playground at http://localhost:{port}/__van/playground");
    eprintln!("  Watching for file changes...");
    eprintln!();

    axum::serve(listener, app).await?;

    Ok(())
}

async fn index_handler(State(state): State<AppState>) -> impl IntoResponse {
    render_page(&state.project, "index")
}

async fn page_handler(
    State(state): State<AppState>,
    Path(page): Path<String>,
) -> impl IntoResponse {
    render_page(&state.project, &page)
}

fn render_page(project: &VanProject, page: &str) -> Html<String> {
    let entry = format!("pages/{page}.van");

    // Collect all source files from src/ and node_modules/
    let files = match project.collect_files() {
        Ok(f) => f,
        Err(e) => return Html(error_html(&format!("Failed to collect files: {e}"))),
    };

    if !files.contains_key(&entry) {
        return Html(not_found_html(page));
    }

    let data = project.load_data(&format!("pages/{page}"));

    // Validate data against defineProps (warning-only)
    if let Some(source) = files.get(&entry) {
        let blocks = van_parser::parse_blocks(source);
        if !blocks.props.is_empty() {
            let label = format!("pages/{page}.van");
            validate_data(&blocks.props, &data, &label);
        }
    }

    match render_from_files(&entry, &files, &data, &HashMap::new()) {
        Ok(html) => Html(html),
        Err(e) => Html(error_html(&format!("{e:#}"))),
    }
}

fn not_found_html(page: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><body>
        <h1>404 — Page not found</h1>
        <p>Could not find <code>src/pages/{page}.van</code></p>
        </body></html>"#
    )
}

fn error_html(message: &str) -> String {
    format!(
        r#"<!DOCTYPE html><html><body>
        <h1>Render Error</h1>
        <pre>{message}</pre>
        </body></html>"#
    )
}

async fn playground_handler() -> Html<&'static str> {
    Html(PLAYGROUND_HTML)
}

/// Serve WASM pkg files for the playground.
///
/// Discovery strategy for the pkg directory:
/// 1. `VAN_WASM_PKG_DIR` environment variable
/// 2. Relative to workspace root: `crates/van-compiler/pkg/`
async fn playground_file_handler(Path(file): Path<String>) -> Response {
    // Only allow specific file extensions
    let content_type = if file.ends_with(".js") {
        "application/javascript"
    } else if file.ends_with(".wasm") {
        "application/wasm"
    } else {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    };

    // Find the pkg directory
    let pkg_dir = if let Ok(dir) = std::env::var("VAN_WASM_PKG_DIR") {
        PathBuf::from(dir)
    } else {
        find_workspace_pkg_dir()
    };

    let file_path = pkg_dir.join(&file);
    match std::fs::read(&file_path) {
        Ok(bytes) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, content_type)],
            bytes,
        )
            .into_response(),
        Err(_) => (
            StatusCode::NOT_FOUND,
            format!(
                "WASM pkg file not found: {}\n\nBuild it with:\n  cd crates/van-compiler && wasm-pack build --target web --features wasm",
                file_path.display()
            ),
        )
            .into_response(),
    }
}

/// Find the van-compiler/pkg directory by walking up from the current exe or cwd.
fn find_workspace_pkg_dir() -> PathBuf {
    // Try relative to current exe
    if let Ok(exe) = std::env::current_exe() {
        let mut dir = exe.parent().map(|p| p.to_path_buf());
        while let Some(d) = dir {
            let candidate = d.join("crates/van-compiler/pkg");
            if candidate.is_dir() {
                return candidate;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Try relative to cwd
    if let Ok(cwd) = std::env::current_dir() {
        let mut dir = Some(cwd);
        while let Some(d) = dir {
            let candidate = d.join("crates/van-compiler/pkg");
            if candidate.is_dir() {
                return candidate;
            }
            dir = d.parent().map(|p| p.to_path_buf());
        }
    }

    // Fallback: relative path (likely won't work, but gives a clear error)
    PathBuf::from("crates/van-compiler/pkg")
}

async fn ws_handler(
    State(state): State<AppState>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state.reload_tx))
}

async fn handle_ws(socket: WebSocket, reload_tx: broadcast::Sender<()>) {
    let mut rx = reload_tx.subscribe();
    let (mut sender, mut receiver) = socket.split();

    // Spawn a task to forward reload signals to the WebSocket client
    let send_task = tokio::spawn(async move {
        while rx.recv().await.is_ok() {
            let msg = Message::Text("{\"type\":\"reload\"}".into());
            if sender.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Drain incoming messages (we don't use them, but need to keep the connection alive)
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(_)) = receiver.next().await {}
    });

    // When either task finishes, abort the other
    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}
