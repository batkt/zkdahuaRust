use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub server:     ServerConfig,
    pub sdk:        SdkConfig,
    pub cameras:    Vec<CameraEntry>,
    #[serde(default)]
    pub sambar_only: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub url:              String,
    pub token:            String,
    #[serde(rename = "barilgiinId")]
    pub barilgiin_id:     String,
    pub timeout_secs:     u64,
    pub retry_count:      u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SdkConfig {
    pub username:                String,
    pub heartbeat_interval_secs: u64,
    pub connect_timeout_ms:      u32,
    pub max_connect_retries:     u32,
    pub port:                    u16,   // default 37777
    pub org_name:                String,
    pub company_name:            String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CameraEntry {
    pub ip:       String,
    pub password: String,
    pub http_port: Option<u16>,
    pub gate:      Option<String>,
    pub sambar_type: Option<String>,
    /// "dahua" or "zk" — plate listener only started for "dahua" cameras
    #[serde(default)]
    pub camera_type: String,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let exe_dir = std::env::current_exe()?
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_owned();
        let cfg_path: PathBuf = exe_dir.join("config.toml");
        let text = std::fs::read_to_string(&cfg_path)
            .map_err(|e| anyhow::anyhow!("Cannot read {}: {e}", cfg_path.display()))?;
        toml::from_str(&text).map_err(|e| anyhow::anyhow!("Config parse error: {e}"))
    }
}
