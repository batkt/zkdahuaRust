//! Posts plate events to the Node.js server.
//! Mirrors C# PlateService.SendPlateDataAsync() — on failure optionally opens gate.

use std::time::Duration;
use log::{warn, error};
use reqwest::Client;
use serde::{Serialize, Deserialize};
use tokio::time::sleep;

use crate::config::ServerConfig;
use crate::camera_manager::{PlateEvent, CAMERA_MANAGER};
use crate::dahua_camera_manager::DAHUA_MANAGER;

// ─── Payload sent to Node.js ──────────────────────────────────────────────────
//
// Matches C# anonymous object: { mashiniiDugaar, CAMERA_IP }

#[derive(Serialize)]
pub struct PlatePayload<'a> {
    pub mashiniiDugaar: &'a str,
    pub CAMERA_IP:      &'a str,
    pub barilgiinId:    &'a str,
}

// ─── Response from Node.js ────────────────────────────────────────────────────

#[derive(Deserialize, Debug, Default)]
pub struct ServerResponse {
    /// true = allow + open gate, false = deny
    pub success:     Option<bool>,
    pub openGate:    Option<bool>,
    pub message:     Option<String>,
}

// ─── PlateService ─────────────────────────────────────────────────────────────

pub struct PlateService {
    client:          Client,
    cfg:             ServerConfig,
}

impl PlateService {
    pub fn new(cfg: ServerConfig) -> anyhow::Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(cfg.timeout_secs))
            .build()?;
        Ok(Self { client, cfg })
    }

    /// Send plate data to the Node.js server.
    /// If the server responds with openGate=true, opens gate immediately.
    /// If the server is unreachable and offline_open_gate=true, opens gate as fallback.
    pub async fn process_plate(&self, event: &PlateEvent) {
        println!("mashinii Dugaar burtgegdlee: {} | IP: {}", event.plate, event.camera_ip);

        let payload = PlatePayload {
            mashiniiDugaar: &event.plate,
            CAMERA_IP:      &event.camera_ip,
            barilgiinId:    &self.cfg.barilgiin_id,
        };

        match self.send_with_retry(&payload).await {
            Ok(response) => {
                println!("Plate {} serverт илгээгдлээ", event.plate);
                // If server explicitly responds with openGate=true, open gate immediately
                // without waiting for a separate frontend /api/neeye call.
                let should_open = response.openGate == Some(true);
                if should_open {
                    println!("Server: хаалга нээх → {}", event.camera_ip);
                    let ok = open_gate_for_camera(&event.camera_ip).await;
                    if !ok {
                        error!("process_plate: open_gate failed for {}", event.camera_ip);
                    }
                }
            }
            Err(e) => {
                error!("Server error for plate {}: {e}", event.plate);
                // Offline fallback: open gate anyway if configured
                if self.cfg.offline_open_gate {
                    warn!("offline_open_gate=true — хаалга offline нөхцөлд нээж байна: {}", event.camera_ip);
                    open_gate_for_camera(&event.camera_ip).await;
                }
            }
        }
    }

    async fn send_with_retry(&self, payload: &PlatePayload<'_>) -> anyhow::Result<ServerResponse> {
        let url = &self.cfg.url;
        let token = &self.cfg.token;
        let max = self.cfg.retry_count;

        for attempt in 0..=max {
            match self.send_once(url, token, payload).await {
                Ok(r) => return Ok(r),
                Err(e) if attempt < max => {
                    let delay = 2_u64.pow(attempt + 1);
                    warn!("Attempt {}/{max} failed: {e}. Retrying in {delay}s", attempt + 1);
                    sleep(Duration::from_secs(delay)).await;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!()
    }

    async fn send_once(&self, url: &str, token: &str, payload: &PlatePayload<'_>)
        -> anyhow::Result<ServerResponse>
    {
        let resp = self.client
            .post(url)
            .bearer_auth(token)
            .json(payload)
            .send()
            .await?;

        let status = resp.status();
        if status.is_success() {
            // Try to deserialize; fall back to default (success=true) if body is not JSON
            let text = resp.text().await.unwrap_or_default();
            let sr: ServerResponse = serde_json::from_str(&text).unwrap_or_default();
            Ok(sr)
        } else {
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("HTTP {status}: {body}")
        }
    }
}

/// Route gate open to the correct manager (ZK or Dahua) based on camera type.
/// Async so Dahua's blocking SDK call runs on a dedicated thread via spawn_blocking.
async fn open_gate_for_camera(ip: &str) -> bool {
    let is_dahua = CAMERA_MANAGER.get()
        .map(|m| m.camera_type_for_ip(ip) == "dahua")
        .unwrap_or(false);

    if is_dahua {
        let ip_owned = ip.to_string();
        tokio::task::spawn_blocking(move || {
            DAHUA_MANAGER.get().map(|m| m.open_gate(&ip_owned)).unwrap_or(false)
        }).await.unwrap_or(false)
    } else {
        // ZK gate_tx send is non-blocking (buffered sync channel)
        CAMERA_MANAGER.get().map(|m| m.open_gate(ip)).unwrap_or(false)
    }
}
