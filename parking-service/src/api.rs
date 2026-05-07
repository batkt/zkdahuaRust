use axum::{
    extract::Path,
    routing::{get, post},
    Router,
    response::IntoResponse,
    http::{StatusCode, Method},
};
use log::info;
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};

use crate::camera_manager::CAMERA_MANAGER;
use crate::dahua_camera_manager::DAHUA_MANAGER;

// ─── Server startup ───────────────────────────────────────────────────────────

pub async fn run_api_server(port: u16) {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/neeye/:ip",             get(neeye))
        .route("/api/sambar/:ip/:text/:dun", get(sambar))
        .route("/api/sambarOgnootoi/:ip/:text/:dun/:start/:end", get(sambar_ognootoi))
        .route("/api/restartConnections",    post(restart_connections))
        .route("/api/health",                get(health))
        .layer(cors);

    let addr = format!("0.0.0.0:{port}");
    info!("API server listening on {addr}");

    let listener = loop {
        match TcpListener::bind(&addr).await {
            Ok(l) => break l,
            Err(_) => {
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    };
    axum::serve(listener, app).await.expect("API server crashed");
}

// ─── Camera type helper ───────────────────────────────────────────────────────

fn camera_type_for_ip(ip: &str) -> &'static str {
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.camera_type_for_ip(ip) == "dahua" {
            return "dahua";
        }
    }
    "zk"
}

// ─── Handlers ────────────────────────────────────────────────────────────────

/// Open barrier gate for a given camera IP.
/// Routes to ZK manager (AlprSDK) or Dahua manager (CLIENT_ControlDevice) based on camera type.
async fn neeye(Path(ip): Path<String>) -> impl IntoResponse {
    if camera_type_for_ip(&ip) == "dahua" {
        println!("dahua >>> neeye | IP: {ip}");
        // Dahua: SDK call is blocking — use spawn_blocking
        let ip_clone = ip.clone();
        let success = tokio::task::spawn_blocking(move || {
            DAHUA_MANAGER.get().map(|m| m.open_gate(&ip_clone)).unwrap_or(false)
        }).await.unwrap_or(false);

        return if success {
            (StatusCode::OK, "Amjilttai".to_string())
        } else {
            log::error!("neeye: Dahua gate open failed for {ip}");
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Dahua gate open failed for {ip}"))
        };
    }

    // ZK camera
    println!("zk >>> neeye | IP: {ip}");
    let mgr = match CAMERA_MANAGER.get() {
        Some(m) => m,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "CameraManager not ready".to_string()),
    };

    match mgr.handle_for_ip(&ip) {
        Some(handle) => {
            let ok = mgr.open_gate(&ip);
            if ok {
                println!("<<< neeye амжилттай | IP: {ip} handle: {handle}");
                (StatusCode::OK, "Amjilttai".to_string())
            } else {
                log::error!("neeye: ZK gate open failed for {ip} (handle {handle})");
                (StatusCode::INTERNAL_SERVER_ERROR, format!("Gate open failed for {ip}"))
            }
        }
        None => {
            // Camera not in handle map yet — may be reconnecting; queue via open_gate
            log::warn!("neeye: ZK camera {ip} not connected — queuing gate open");
            let queued = mgr.open_gate(&ip);
            if queued {
                (StatusCode::OK, "Queued".to_string())
            } else {
                (StatusCode::SERVICE_UNAVAILABLE, format!("Camera {ip} not connected"))
            }
        }
    }
}

/// LED screen display.
/// ZK: uses AlprSDK_Trans2Screen.
/// Dahua: uses HTTP configManager.cgi.
async fn sambar(Path((ip, text, dun)): Path<(String, String, String)>) -> impl IntoResponse {
    if camera_type_for_ip(&ip) == "dahua" {
        println!("dahua >>> sambar | IP: {ip} text: {text} dun: {dun}");
        return dahua_sambar(&ip, &text, &dun).await;
    }

    println!("zk >>> sambar | IP: {ip} text: {text} dun: {dun}");
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.handle_for_ip(&ip).is_some() {
            let ok = mgr.display_on_screen(&ip, &text, &dun);
            return if ok {
                (StatusCode::OK, "Amjilttai".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string())
            };
        }
    }
    (StatusCode::SERVICE_UNAVAILABLE, format!("Camera {ip} not connected"))
}

async fn sambar_ognootoi(
    Path((ip, text, dun, start, end)): Path<(String, String, String, String, String)>,
) -> impl IntoResponse {
    if camera_type_for_ip(&ip) == "dahua" {
        println!("dahua >>> sambarOgnootoi | IP: {ip}");
        return dahua_sambar_ognootoi(&ip, &text, &dun, &start, &end).await;
    }

    println!("zk >>> sambarOgnootoi | IP: {ip}");
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.handle_for_ip(&ip).is_some() {
            let ok = mgr.display_on_screen_ognootoi(&ip, &text, &dun, &start, &end);
            return if ok {
                (StatusCode::OK, "Amjilttai".to_string())
            } else {
                (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string())
            };
        }
    }
    (StatusCode::SERVICE_UNAVAILABLE, format!("Camera {ip} not connected"))
}

async fn restart_connections() -> impl IntoResponse {
    info!("Manual connection restart requested via API");
    tokio::task::spawn_blocking(|| {
        if let Some(mgr) = CAMERA_MANAGER.get() {
            mgr.connect_all();
        }
    });
    let zk_count    = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    let dahua_count = DAHUA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("Restart initiated. ZK={zk_count} Dahua={dahua_count}"))
}

async fn health() -> impl IntoResponse {
    let zk_count    = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    let dahua_count = DAHUA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("OK | zk={zk_count} dahua={dahua_count}"))
}

// ─── Dahua HTTP sambar helpers ────────────────────────────────────────────────

fn dahua_password_for_ip(ip: &str) -> String {
    // Look up in ZK manager's cam_cfg (which holds ALL cameras including Dahua)
    CAMERA_MANAGER.get()
        .and_then(|m| m.password_for_ip(ip))
        .unwrap_or_else(|| "admin123".to_string())
}

fn dahua_is_entrance(ip: &str) -> bool {
    CAMERA_MANAGER.get()
        .map(|m| m.gate_for_ip(ip).to_lowercase() == "entrance")
        .unwrap_or(false)
}

fn dahua_org_name() -> String {
    // Use CAMERA_MANAGER's config
    CAMERA_MANAGER.get()
        .map(|m| m.org_name().to_string())
        .unwrap_or_else(|| "ParkEase".to_string())
}

fn dahua_company_name() -> String {
    CAMERA_MANAGER.get()
        .map(|m| m.company_name().to_string())
        .unwrap_or_else(|| "ParkEase".to_string())
}

async fn dahua_sambar(ip: &str, text: &str, dun: &str) -> (StatusCode, String) {
    let password     = dahua_password_for_ip(ip);
    let is_entrance  = dahua_is_entrance(ip);
    let org_name     = dahua_org_name();
    let company_name = dahua_company_name();

    let dun_t  = format!("{}T", dun);
    let line1  = if is_entrance { org_name.clone() } else { dun_t.clone() };
    let line2  = if is_entrance { "Төлбөртэй зогсоол".to_string() } else { org_name.clone() };
    let carpass_0 = if is_entrance { company_name } else { "".to_string() };

    let params = [
        "TrafficLatticeScreen[0].StatusChangeTime=1".to_string(),
        format!("TrafficLatticeScreen[0].Normal.Contents.[0]=str({text})"),
        format!("TrafficLatticeScreen[0].Normal.Contents.[1]=str({line1})"),
        format!("TrafficLatticeScreen[0].Normal.Contents.[2]=str({line2})"),
        format!("TrafficLatticeScreen[0].CarPass.Contents.[0]=str({carpass_0})"),
        "TrafficLatticeScreen[0].CarPass.Contents.[1]=SysTime".to_string(),
    ];

    let url = format!(
        "http://{ip}/cgi-bin/configManager.cgi?action=setConfig&{}",
        params.join("&")
    );

    match send_dahua_http(&url, &password).await {
        Ok(_)  => (StatusCode::OK, "Amjilttai".to_string()),
        Err(e) => { println!("dahua sambar aldaa: {e}"); (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string()) }
    }
}

async fn dahua_sambar_ognootoi(ip: &str, text: &str, dun: &str, start: &str, end: &str) -> (StatusCode, String) {
    let password = dahua_password_for_ip(ip);
    let dun_t    = format!("{}T", dun);

    let params = [
        "TrafficLatticeScreen[0].StatusChangeTime=1".to_string(),
        format!("TrafficLatticeScreen[0].Normal.Contents.[0]=str({text})"),
        format!("TrafficLatticeScreen[0].Normal.Contents.[1]=str({dun_t})"),
        format!("TrafficLatticeScreen[0].Normal.Contents.[2]=str({start})"),
        format!("TrafficLatticeScreen[0].Normal.Contents.[3]=str({end})"),
        format!("TrafficLatticeScreen[0].CarPass.Contents.[0]=str({text})"),
        format!("TrafficLatticeScreen[0].CarPass.Contents.[1]=str({dun_t})"),
        "TrafficLatticeScreen[0].CarPass.Contents.[2]=str()".to_string(),
        "TrafficLatticeScreen[0].CarPass.Contents.[3]=SysTime".to_string(),
    ];

    let url = format!(
        "http://{ip}/cgi-bin/configManager.cgi?action=setConfig&{}",
        params.join("&")
    );

    match send_dahua_http(&url, &password).await {
        Ok(_)  => (StatusCode::OK, "Amjilttai".to_string()),
        Err(e) => { println!("dahua sambarOgnootoi aldaa: {e}"); (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string()) }
    }
}

async fn send_dahua_http(url: &str, password: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .danger_accept_invalid_certs(true)
        .build()?;

    let first = client.get(url).send().await?;

    let resp = if first.status() == reqwest::StatusCode::UNAUTHORIZED {
        let auth_header = first.headers()
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();

        let uri = if let Some(pos) = url.find("/cgi-bin") { &url[pos..] } else { url };
        let mut prompt = digest_auth::parse(&auth_header)?;
        let context = digest_auth::AuthContext::new("admin", password, uri);
        let answer = prompt.respond(&context)?.to_header_string();
        client.get(url).header("Authorization", answer).send().await?
    } else {
        first
    };

    Ok(resp.text().await.unwrap_or_default())
}
