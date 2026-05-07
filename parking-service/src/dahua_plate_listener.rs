//! Dahua camera plate detection via HTTP multipart stream

use std::collections::HashMap;
use std::time::Instant;
use std::sync::Mutex;
use log::{info, warn};
use once_cell::sync::Lazy;
use tokio::sync::mpsc;
use digest_auth;

use crate::camera_manager::PlateEvent;

// Debounce: same plate from same camera within 10 seconds = skip
static LAST_PLATE: Lazy<Mutex<HashMap<String, Instant>>> = Lazy::new(|| Mutex::new(HashMap::new()));

pub async fn run_plate_listener(ip: String, password: String, port: u16, plate_tx: mpsc::Sender<PlateEvent>) {
    let scheme = if port == 443 { "https" } else { "http" };
    let host = if (scheme == "http" && port == 80) || (scheme == "https" && port == 443) {
        ip.clone()
    } else {
        format!("{ip}:{port}")
    };
    let url = format!(
        "{scheme}://{host}/cgi-bin/snapManager.cgi?action=attachFileProc&Flags[0]=Event&Events=[TrafficJunction]&heartbeat=5"
    );

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .expect("reqwest client build failed");

    let mut error_streak: u32 = 0;

    loop {
        if error_streak > 0 {
            let delay_secs = std::cmp::min(2u64.pow(error_streak.min(5)), 30);
            info!("[Dahua {ip}] Алдааны дараа {delay_secs}s хүлээж байна (#{error_streak})");
            tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
        }

        info!("[Dahua {ip}] Plate listener холбогдож байна...");

        match connect_and_read(&client, &ip, &password, &url, &plate_tx).await {
            Ok(_) => {
                error_streak = 0;
                info!("[Dahua {ip}] Stream дууссан, шууд дахин холбогдоно...");
            }
            Err(e) => {
                error_streak += 1;
                warn!("[Dahua {ip}] Холболтын алдаа (#{error_streak}): {e}");
            }
        }
    }
}

async fn connect_and_read(
    client: &reqwest::Client,
    ip: &str,
    password: &str,
    url: &str,
    plate_tx: &mpsc::Sender<PlateEvent>,
) -> anyhow::Result<()> {
    let first = client.get(url).send().await?;

    let resp = if first.status() == reqwest::StatusCode::UNAUTHORIZED {
        let auth_header = first.headers()
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let mut prompt = digest_auth::parse(&auth_header)?;
        let context = digest_auth::AuthContext::new("admin", password, url);
        let answer = prompt.respond(&context)?.to_header_string();

        client.get(url).header("Authorization", answer).send().await?
    } else {
        first
    };

    if !resp.status().is_success() {
        anyhow::bail!("HTTP {}", resp.status());
    }

    info!("[Dahua {ip}] Амжилттай холбогдлоо — plate event хүлээж байна...");

    let mut stream = resp.bytes_stream();
    let mut buffer = String::new();

    use tokio_stream::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer[..newline_pos].trim().to_string();
            buffer = buffer[newline_pos + 1..].to_string();

            if line.starts_with("Events[0].Object.Text") {
                let plate = line
                    .replace("Events[0].Object.Text=", "")
                    .trim()
                    .trim_end_matches('\0')
                    .to_string();

                if plate.is_empty() { continue; }

                let key = format!("{ip}:{plate}");
                {
                    let now = Instant::now();
                    let mut map = LAST_PLATE.lock().unwrap();
                    if let Some(last) = map.get(&key) {
                        if now.duration_since(*last).as_secs() < 10 {
                            info!("[Dahua {ip}] Давхардсан plate давсан: {plate}");
                            continue;
                        }
                    }
                    map.insert(key, now);
                }

                info!(">>> Dahua Plate бүртгэгдлээ: {plate} | IP: {ip}");

                // handle = -1 signals this is a Dahua event (not ZK)
                let event = PlateEvent { plate, camera_ip: ip.to_string(), handle: -1 };
                if let Err(e) = plate_tx.send(event).await {
                    warn!("[Dahua {ip}] Plate channel алдаа: {e}");
                }
            }
        }
    }

    Ok(())
}
