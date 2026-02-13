use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::{fs, path::Path};

#[derive(Debug, Deserialize, Serialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Release {
    tag_name: String,
    name: Option<String>,
    body: Option<String>,
    assets: Vec<Asset>,
}

async fn fetch_latest_release(repo: &str) -> anyhow::Result<Release> {
    let url = format!("https://api.github.com/repos/{}/releases/latest", repo);
    let client = Client::new();
    let release: Release = client
        .get(&url)
        .header("User-Agent", "rust-release-notifier")
        .send()
        .await?
        .json()
        .await?;
    Ok(release)
}

#[derive(Debug, Deserialize, Serialize)]
struct PrevVersion {
    repos: HashMap<String, String>, // repo -> last seen tag
}

fn load_prev(path: &str) -> PrevVersion {
    if std::path::Path::new(path).exists() {
        let data = std::fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_else(|_| PrevVersion {
            repos: Default::default(),
        })
    } else {
        PrevVersion {
            repos: Default::default(),
        }
    }
}

fn save_prev(path: &str, prev: &PrevVersion) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(prev)?;
    std::fs::write(path, json)?;
    Ok(())
}

async fn process_repos(repos: Vec<&str>) -> anyhow::Result<()> {
    let prev_path = "prev.json";
    let mut prev = load_prev(prev_path);

    for repo in repos {
        let release = fetch_latest_release(repo).await?;

        // Skip if already processed
        if let Some(last_tag) = prev.repos.get(repo) {
            if last_tag == &release.tag_name {
                println!("No new release for {} ({}). Skipping.", repo, last_tag);
                continue;
            }
        }

        println!("New release found for {}: {}", repo, release.tag_name);

        download_assets_concurrent(repo, &release).await?;

        // update prev.json for this repo
        prev.repos
            .insert(repo.to_string(), release.tag_name.clone());
    }

    save_prev(prev_path, &prev)?;
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let repos = vec![
        "NoName-exe/revanced-extended",
        "ReadYouApp/ReadYou",
        "ProtonMail/android-mail",
        "Helium314/HeliBoard",
        "Akylas/OSS-DocumentScanner",
        "foobnix/LibreraReader",
    ];

    process_repos(repos).await?;

    Ok(())
}

async fn download_asset(
    client: &Client,
    url: &str,
    dir: &str,
    filename: &str,
) -> anyhow::Result<()> {
    let resp = client.get(url).send().await?;
    let bytes = resp.bytes().await?;
    let path = Path::new(dir).join(filename);
    tokio::fs::write(&path, &bytes).await?;
    Ok(())
}

async fn download_assets_concurrent(repo: &str, release: &Release) -> anyhow::Result<Vec<String>> {
    let client = Client::new();
    let dir = format!("assets/{}", repo.replace("/", "_"));
    tokio::fs::create_dir_all(&dir).await?;

    // Clear previous list.txt before downloads
    let list_path = "assets/list.txt";
    tokio::fs::write(list_path, "").await.unwrap_or(());

    let mut tasks = Vec::new();
    let mut file_paths = Vec::new();

    for asset in &release.assets {
        if asset.name.contains("magisk") || asset.name.contains("arm-v7a") {
            continue;
        }
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

    // Write the file list
    let mut list_file = fs::File::create("assets/list.txt")?;
    for path in &file_paths {
        list_file.write_all(path.as_bytes())?;
        list_file.write_all(b"\n")?;
    }

    Ok(file_paths)
}
