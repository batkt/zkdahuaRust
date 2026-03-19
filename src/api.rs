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
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};

use crate::camera_manager::CAMERA_MANAGER;

pub async fn run_api_server(port: u16) {
    let cors = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_origin(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/neeye/:ip",             get(neeye))
        .route("/api/sambar/:ip/:text/:dun", get(sambar))
        .route("/api/restartConnections",    post(restart_connections))
        .route("/api/health",                get(health))
        .fallback(handler_404)
        .layer(cors);

    let addr = format!("0.0.0.0:{port}");
    info!("API server listening on {addr}");

    let listener = TcpListener::bind(&addr).await.expect("Cannot bind API port");
    axum::serve(listener, app).await.expect("API server crashed");
}

/// Open barrier gate — called by frontend after server approves plate
async fn neeye(Path(ip): Path<String>) -> impl IntoResponse {
    println!("neeye called for ip: {ip}");
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.handle_for_ip(&ip).is_some() {
            mgr.open_gate(&ip);
            return (StatusCode::OK, "Amjilttai".to_string());
        }
    }
    println!("neeye Aldaa: IP {ip} not found");
    (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string())
}

/// LED screen display — HTTP configManager.cgi
async fn sambar(Path((ip, text, dun)): Path<(String, String, String)>) -> impl IntoResponse {
    println!("sambar called for ip: {ip} text: {text} dun: {dun}");

    let (password, is_entrance, org_name, company_name) = CAMERA_MANAGER
        .get()
        .map(|m| (m.password_for_ip(&ip), m.is_entrance(&ip), m.org_name().to_string(),   m.company_name().to_string(),))
       .unwrap_or(("admin123".to_string(), false, "ParkEase".to_string(), "ParkEase".to_string()));

    let dun_t = format!("{}T", dun);
    let line1 = if is_entrance { org_name.clone() } else { dun_t.clone() };
    let line2 = if is_entrance { "Төлбөртэй зогсоол".to_string() } else { org_name.clone() };
    let carpass_0 = if is_entrance { company_name.clone() } else { "".to_string() };
    let params = [
        "TrafficLatticeScreen[0].StatusChangeTime=1".to_string(),
        format!("TrafficLatticeScreen[0].Normal.Contents[0]=str({text})"),
        format!("TrafficLatticeScreen[0].Normal.Contents[1]=str({line1})"), 
        format!("TrafficLatticeScreen[0].Normal.Contents[2]=str({line2})"),
        format!("TrafficLatticeScreen[0].CarPass.Contents[0]=str({carpass_0})"),
        "TrafficLatticeScreen[0].CarPass.Contents[1]=SysTime".to_string(),
    ];

    let url = format!(
        "http://{ip}/cgi-bin/configManager.cgi?action=setConfig&{}",
        params.join("&")
    );

    println!("[SAMBAR] URL: {url}");

    match send_sambar_request(&url, &password).await {
        Ok(body) => {
            println!("sambar response: {body}");
            (StatusCode::OK, "Amjilttai".to_string())
        }
        Err(e) => {
            println!("sambar Aldaa: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "aldaa".to_string())
        }
    }
}
async fn send_sambar_request(url: &str, password: &str) -> anyhow::Result<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()?;

    // Use reqwest::Url directly to avoid re-encoding
    let parsed = reqwest::Url::parse(url)?;

    let first = client.get(parsed.clone()).send().await?;
    println!("First response status: {}", first.status());

    let resp = if first.status() == reqwest::StatusCode::UNAUTHORIZED {
        let auth_header = first.headers()
            .get("WWW-Authenticate")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string();
        println!("WWW-Authenticate: {auth_header}");

        let mut prompt = digest_auth::parse(&auth_header)?;
        let context = digest_auth::AuthContext::new("admin", password, url);
        let answer = prompt.respond(&context)?.to_header_string();

        client.get(parsed).header("Authorization", answer).send().await?
    } else {
        first
    };

    println!("Final response status: {}", resp.status());
    let body = resp.text().await.unwrap_or_default();
    Ok(body)
}

async fn restart_connections() -> impl IntoResponse {
    info!("Manual connection restart requested via API");
    tokio::task::spawn_blocking(|| {
        if let Some(mgr) = CAMERA_MANAGER.get() {
            mgr.reconnect_all();
        }
    });
    let count = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("Restart initiated. {count} camera(s) in list"))
}

async fn health() -> impl IntoResponse {
    let count = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("OK | cameras={count}"))
}

async fn handler_404(req: axum::extract::Request) -> impl IntoResponse {
    println!("404 Not Found: {} {}", req.method(), req.uri());
    (StatusCode::NOT_FOUND, "Not Found")
}
