use reqwest::Client;
use serde::{Deserialize, Serialize};
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

#[derive(Debug, Deserialize, Serialize)]
struct Prev {
    tag_name: String,
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

fn load_prev(path: &str) -> Option<Prev> {
    if Path::new(path).exists() {
        let data = fs::read_to_string(path).ok()?;
        serde_json::from_str(&data).ok()
    } else {
        None
    }
}

fn save_prev(path: &str, release: &Release) -> anyhow::Result<()> {
    let prev = Prev {
        tag_name: release.tag_name.clone(),
    };
    let json = serde_json::to_string_pretty(&prev)?;
    fs::write(path, json)?;
    Ok(())
}

/// Helper: get Content-Length of an asset
async fn fetch_asset_size(client: &Client, url: &str) -> anyhow::Result<u64> {
    let resp = client.head(url).send().await?;
    if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
        Ok(len.to_str()?.parse::<u64>()?)
    } else {
        Ok(0) // fallback if GitHub hides size
    }
}

/// Old single-release notifier
async fn send_to_telegram(token: &str, chat_id: &str, release: &Release) -> anyhow::Result<()> {
    let mut message = format!("🚀 New Release: *{}*\n\n", release.tag_name);

    for asset in &release.assets {
        message.push_str(&format!("🔗 [{}]({})\n", asset.name, asset.browser_download_url));
        message.push_str(&format!(
            "\n🧲 curl command:\n```\ncurl -L --http1.1 -A \"Mozilla/5.0\" -o {} {}\n```\n",
            asset.name, asset.browser_download_url
        ));
    }

    let url = format!("https://api.telegram.org/bot{}/sendMessage", token);

    let client = Client::new();
    client
        .post(&url)
        .form(&[
            ("chat_id", chat_id),
            ("text", &message),
            ("parse_mode", "Markdown"),
        ])
        .send()
        .await?;

    Ok(())
}

/// New multi-repo handler
async fn process_repos(token: &str, chat_id: &str, repos: Vec<&str>) -> anyhow::Result<()> {
    let client = Client::new();

    for repo in repos {
        let release = fetch_latest_release(repo).await?;
        let mut message = format!("🚀 New Release from *{}*: *{}*\n\n", repo, release.tag_name);

        for asset in &release.assets {
            let size = fetch_asset_size(&client, &asset.browser_download_url).await.unwrap_or(0);
            if size > 0 && size <= 50 * 1024 * 1024 {
                // under 50MB: upload as file
                let url = format!("https://api.telegram.org/bot{}/sendDocument", token);
                let form = reqwest::multipart::Form::new()
                    .text("chat_id", chat_id.to_string())
                    .part(
                        "document",
                        reqwest::multipart::Part::stream(
                            client.get(&asset.browser_download_url).send().await?.bytes().await?,
                        )
                        .file_name(asset.name.clone()),
                    )
                    .text("caption", format!("{} ({:.2} MB)", asset.name, size as f64 / 1024.0 / 1024.0));
                client.post(&url).multipart(form).send().await?;
            } else {
                // over 50MB: just send link + curl
                message.push_str(&format!("🔗 [{}]({})\n", asset.name, asset.browser_download_url));
                message.push_str(&format!(
                    "\n🧲 curl command:\n```\ncurl -L --http1.1 -A \"Mozilla/5.0\" -o {} {}\n```\n",
                    asset.name, asset.browser_download_url
                ));
            }
        }

        // send text message if we didn’t upload all assets as files
        if !message.trim().is_empty() {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            client
                .post(&url)
                .form(&[
                    ("chat_id", chat_id),
                    ("text", &message),
                    ("parse_mode", "Markdown"),
                ])
                .send()
                .await?;
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN")?;
    let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID")?;

    let repos = vec![
        "NoName-exe/revanced-extended",
        "rust-lang/rust", // just example
    ];

    process_repos(&telegram_token, &telegram_chat_id, repos).await?;

    Ok(())
}