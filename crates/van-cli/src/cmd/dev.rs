use anyhow::Result;

pub async fn run() -> Result<()> {
    van_dev::start(3000).await
}
