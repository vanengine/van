use anyhow::Result;
use notify::{Event, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

/// Start watching the `src/` and `mock/` directories for file changes.
///
/// When a `.van`, `.json`, or `.css` file changes, increments the version counter
/// and sends a notification through the broadcast channel.
pub fn start(
    project_dir: &Path,
    version: Arc<AtomicU64>,
    tx: broadcast::Sender<()>,
) -> Result<impl Watcher> {
    let src_dir = project_dir.join("src");
    let mock_dir = project_dir.join("mock");

    let mut watcher =
        notify::recommended_watcher(move |res: std::result::Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let dominated = event.paths.iter().any(|p| {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
                    matches!(ext, "van" | "json" | "css")
                });
                if dominated {
                    version.fetch_add(1, Ordering::SeqCst);
                    let _ = tx.send(());
                }
            }
        })?;

    if src_dir.exists() {
        watcher.watch(&src_dir, RecursiveMode::Recursive)?;
    }
    if mock_dir.exists() {
        watcher.watch(&mock_dir, RecursiveMode::Recursive)?;
    }

    Ok(watcher)
}
