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

async fn send_to_telegram(token: &str, chat_id: &str, release: &Release) -> anyhow::Result<()> {
    let mut message = format!("🚀 New Release: *{}*\n\n", release.tag_name);

    for asset in &release.assets {
        message.push_str(&format!("🔗 [{}]({})\n", asset.name, asset.browser_download_url));

/* curl -L --http1.1 -A "Mozilla/5.0" -o youtube-revanced-extended-v19.47.53-all.apk \
  https://github.com/NoName-exe/revanced-extended/releases/download/136/youtube-revanced-extended-v19.47.53-all.apk */
        message.push_str(&format!(
    "\n\n🧲 curl command:\n```\ncurl -L --http1.1 -A \"Mozilla/5.0\" -o {} {}\n```\n",
    asset.name,
    asset.browser_download_url
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
    let repo = "NoName-exe/revanced-extended";
    let prev_path = "prev.json";

    // Secrets from env vars
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN")?;
    let telegram_chat_id = std::env::var("TELEGRAM_CHAT_ID")?;

    let latest = fetch_latest_release(repo).await?;

    let prev = load_prev(prev_path);

    if let Some(p) = prev {
        if p.tag_name == latest.tag_name {
            println!("No new release. Exiting.");
            return Ok(());
        }
    }

    // Save and notify
    save_prev(prev_path, &latest)?;
    send_to_telegram(&telegram_token, &telegram_chat_id, &latest).await?;

    println!("Notified about new release: {}", latest.tag_name);
    Ok(())
}
