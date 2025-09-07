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

use reqwest::Client;
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct Prev {
    repos: HashMap<String, String>, // repo -> last seen tag
}

fn load_prev(path: &str) -> Prev {
    if std::path::Path::new(path).exists() {
        let data = std::fs::read_to_string(path).unwrap_or_default();
        serde_json::from_str(&data).unwrap_or_else(|_| Prev { repos: Default::default() })
    } else {
        Prev { repos: Default::default() }
    }
}

fn save_prev(path: &str, prev: &Prev) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(prev)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Get Content-Length of an asset
async fn fetch_asset_size(client: &Client, url: &str) -> anyhow::Result<u64> {
    let resp = client.head(url).send().await?;
    if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
        Ok(len.to_str()?.parse::<u64>()?)
    } else {
        Ok(0)
    }
}

async fn process_repos(token: &str, chat_id: &str, repos: Vec<&str>) -> anyhow::Result<()> {
    let client = Client::new();
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

        let mut message = format!("🚀 New Release from *{}*: *{}*\n\n", repo, release.tag_name);
        let mut sent_text = false;

        for asset in &release.assets {
            let size = fetch_asset_size(&client, &asset.browser_download_url).await.unwrap_or(0);

            if size > 0 && size <= 50 * 1024 * 1024 {
                // send as Telegram file
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
                // add to text message
                message.push_str(&format!("🔗 [{}]({})\n", asset.name, asset.browser_download_url));
                message.push_str(&format!(
                    "\n🧲 curl command:\n```\ncurl -L --http1.1 -A \"Mozilla/5.0\" -o {} {}\n```\n",
                    asset.name, asset.browser_download_url
                ));
                sent_text = true;
            }
        }

        // Send text message if needed
        if sent_text {
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

        // update prev.json for this repo
        prev.repos.insert(repo.to_string(), release.tag_name.clone());
    }

    save_prev(prev_path, &prev)?;
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN")?;
    let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID")?;

    let repos = vec![
        "NoName-exe/revanced-extended",
        "ReadYouApp/ReadYou", // just example
    ];

    process_repos(&telegram_token, &telegram_chat_id, repos).await?;

    Ok(())
}