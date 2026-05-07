use serde::Deserialize;
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server:  ServerConfig,
    pub cameras: Vec<CameraEntry>,
    pub sdk:     SdkConfig,
    /// Run API only, skip camera SDK init (useful for display-only nodes)
    #[serde(default)]
    pub sambar_only: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Node.js server URL
    pub url: String,
    /// JWT bearer token
    pub token: String,
    /// HTTP timeout (seconds)
    pub timeout_secs: u64,
    /// Retries on failure
    pub retry_count: u32,
    /// Building ID sent with every plate payload
    #[serde(rename = "barilgiinId")]
    pub barilgiin_id: String,
    /// If true, open gate even when server is unreachable (fail-open)
    #[serde(default)]
    pub offline_open_gate: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CameraEntry {
    /// Camera IP
    pub ip: String,
    /// Web/HTTP port — ZK uses HTTPS (443), Dahua uses HTTP (80 or 443)
    /// Accepts both `port` and `http_port` keys in TOML for backward compatibility
    #[serde(alias = "http_port", default = "default_port")]
    pub port: u16,
    /// Login password
    pub password: String,
    /// Gate role: "entrance" or "exit"
    #[serde(default)]
    pub gate: String,
    /// Camera type: "zk" or "dahua" (default "zk")
    #[serde(default)]
    pub camera_type: String,
    /// Dahua-specific: sambar display type
    pub sambar_type: Option<String>,
}

fn default_port() -> u16 { 443 }

#[derive(Debug, Deserialize, Clone)]
pub struct SdkConfig {
    /// Login username (always "admin")
    pub username: String,

    // ── ZK-specific fields ──────────────────────────────────────────────
    /// Camera search interval ms
    #[serde(default = "default_search_interval_ms")]
    pub search_interval_ms: u32,
    /// Heartbeat interval seconds
    #[serde(default = "default_heartbeat_interval")]
    pub heartbeat_interval_secs: u64,
    /// Connection timeout base ms
    #[serde(default = "default_connect_timeout")]
    pub connect_timeout_ms: u32,
    /// Max connection retries per camera
    #[serde(default = "default_max_retries")]
    pub max_connect_retries: u32,

    // ── Dahua-specific fields ───────────────────────────────────────────
    /// Dahua SDK TCP port (default 37777)
    #[serde(default = "default_dahua_sdk_port")]
    pub dahua_sdk_port: u16,
    /// Organisation name shown on Dahua LED screen
    #[serde(default = "default_org_name")]
    pub org_name: String,
    /// Company name shown on Dahua LED screen
    #[serde(default = "default_company_name")]
    pub company_name: String,
}

fn default_search_interval_ms() -> u32 { 3000 }
fn default_heartbeat_interval()  -> u64 { 30 }
fn default_connect_timeout()     -> u32 { 5000 }
fn default_max_retries()         -> u32 { 5 }
fn default_dahua_sdk_port()      -> u16 { 37777 }
fn default_org_name()            -> String { "ParkEase".to_string() }
fn default_company_name()        -> String { "ParkEase".to_string() }

impl Config {
    pub fn load() -> Result<Self> {
        let exe_dir = std::env::current_exe()?
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_owned();
        let path = exe_dir.join("config.toml");

        let text = fs::read_to_string(&path)
            .with_context(|| format!("Cannot read {}", path.display()))?;

        toml::from_str(&text).context("Failed to parse config.toml")
    }
}
