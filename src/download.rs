use std::path::Path;
use tokio::fs;
use reqwest::Client;

async fn download_asset(client: &Client, url: &str, dir: &str, filename: &str) -> anyhow::Result<()> {
    let resp = client.get(url).send().await?;
    let bytes = resp.bytes().await?;
    let path = Path::new(dir).join(filename);
    fs::write(&path, &bytes).await?;
    Ok(())
}

async fn download_assets_concurrent(repo: &str, release: &Release) -> anyhow::Result<Vec<String>> {
    let client = Client::new();
    let dir = format!("assets/{}", repo.replace("/", "_"));
    fs::create_dir_all(&dir).await?;

    let mut tasks = Vec::new();
    let mut file_paths = Vec::new();

    for asset in &release.assets {
        let url = asset.browser_download_url.clone();
        let filename = asset.name.clone();
        let path = format!("{}/{}", dir, filename);
        file_paths.push(path.clone());

        let client = client.clone();
        let dir = dir.clone();

        tasks.push(tokio::spawn(async move {
            download_asset(&client, &url, &dir, &filename).await
        }));
    }

    // Wait for all downloads
    for t in tasks {
        t.await??;
    }

    Ok(file_paths)
}