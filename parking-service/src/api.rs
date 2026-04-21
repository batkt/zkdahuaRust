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

const DAHUA_PORT: u16 = 5001;

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

/// Forward an unknown IP's request to the Dahua service at port 5001.
async fn forward(path: &str) -> (StatusCode, String) {
    let url = format!("http://127.0.0.1:{DAHUA_PORT}{path}");
    println!("    → forwarding to Dahua service: {url}");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .unwrap_or_default();
    match client.get(&url).send().await {
        Ok(resp) => {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            (StatusCode::from_u16(status.as_u16()).unwrap_or(StatusCode::OK), body)
        }
        Err(e) => {
            println!("    forward aldaa: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "forward aldaa".to_string())
        }
    }
}

async fn neeye(Path(ip): Path<String>) -> impl IntoResponse {
    let mgr = match CAMERA_MANAGER.get() {
        Some(m) => m,
        None => return (StatusCode::INTERNAL_SERVER_ERROR, "CameraManager not ready".to_string()),
    };

    if mgr.camera_type_for_ip(&ip) == "dahua" {
        println!("dahua >>> neeye | IP: {ip}");
        return forward(&format!("/api/neeye/{ip}")).await;
    }

    println!("zk >>> neeye | IP: {ip}");
    match mgr.handle_for_ip(&ip) {
        Some(handle) => {
            mgr.open_gate(&ip);
            println!("<<< neeye амжилттай | IP: {ip} handle: {handle}");
            (StatusCode::OK, "Amjilttai".to_string())
        }
        None => forward(&format!("/api/neeye/{ip}")).await,
    }
}

async fn sambar(Path((ip, text, dun)): Path<(String, String, String)>) -> impl IntoResponse {
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.camera_type_for_ip(&ip) == "dahua" {
            println!("dahua >>> sambar | IP: {ip} text: {text} dun: {dun}");
            return forward(&format!("/api/sambar/{ip}/{text}/{dun}")).await;
        }
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
    forward(&format!("/api/sambar/{ip}/{text}/{dun}")).await
}

async fn sambar_ognootoi(
    Path((ip, text, dun, start, end)): Path<(String, String, String, String, String)>,
) -> impl IntoResponse {
    if let Some(mgr) = CAMERA_MANAGER.get() {
        if mgr.camera_type_for_ip(&ip) == "dahua" {
            println!("dahua >>> sambarOgnootoi | IP: {ip} text: {text} dun: {dun} start: {start} end: {end}");
            return forward(&format!("/api/sambarOgnootoi/{ip}/{text}/{dun}/{start}/{end}")).await;
        }
    }
    println!("zk >>> sambarOgnootoi | IP: {ip} text: {text} dun: {dun} start: {start} end: {end}");
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
    forward(&format!("/api/sambarOgnootoi/{ip}/{text}/{dun}/{start}/{end}")).await
}

async fn restart_connections() -> impl IntoResponse {
    info!("Manual connection restart requested via API");
    tokio::task::spawn_blocking(|| {
        if let Some(mgr) = CAMERA_MANAGER.get() {
            mgr.connect_all();
        }
    });
    let count = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("Restart initiated. {count} camera(s) in list"))
}

async fn health() -> impl IntoResponse {
    let count = CAMERA_MANAGER.get().map(|m| m.camera_count()).unwrap_or(0);
    (StatusCode::OK, format!("OK | cameras={count}"))
}
