pub mod render;
mod server;
mod watcher;

pub async fn start(port: u16) -> anyhow::Result<()> {
    server::run(port).await
}
