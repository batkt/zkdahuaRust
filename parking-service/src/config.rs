use serde::Deserialize;
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server: ServerConfig,
    pub cameras: Vec<CameraEntry>,
    pub sdk: SdkConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    /// Node.js server URL — e.g. "http://103.143.40.230:8081"
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
    /// Camera IP (e.g. "192.168.0.11")
    pub ip: String,
    /// Web port: 80 or 443 depending on firmware
    #[serde(rename = "http_port", default = "default_port")]
    pub port: u16,
    /// Login password (default "123456" in C# code)
    pub password: String,
    /// Gate role: "entrance" or "exit"
    pub gate: String,
    /// Camera type: "zk" or "dahua" (default "zk")
    #[serde(default)]
    pub camera_type: String,
}

fn default_port() -> u16 { 443 }
fn default_search_interval_ms() -> u32 { 3000 }

#[derive(Debug, Deserialize, Clone)]
pub struct SdkConfig {
    /// Login username (always "admin")
    pub username: String,
    /// Camera search interval ms (3000 in C# code)
    #[serde(default = "default_search_interval_ms")]
    pub search_interval_ms: u32,
    /// Heartbeat interval seconds
    pub heartbeat_interval_secs: u64,
    /// Connection timeout base ms
    pub connect_timeout_ms: u32,
    /// Max connection retries per camera
    pub max_connect_retries: u32,
}

impl Default for SdkConfig {
    fn default() -> Self {
        Self {
            username: "admin".into(),
            search_interval_ms: 3000,
            heartbeat_interval_secs: 30,
            connect_timeout_ms: 5000,
            max_connect_retries: 5,
        }
    }
}

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
