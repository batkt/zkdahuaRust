//! dahua-service.exe — Rust port of C# Dahua camera parking service
//!
//! Usage:
//!   dahua-service.exe install    — install as Windows service
//!   dahua-service.exe uninstall  — remove Windows service
//!   dahua-service.exe run        — run interactively (debug)
//!   (no args)                    — called by Windows SCM

mod sdk;
mod config;
mod camera_manager;
mod plate_listener;
mod plate_service;
mod api;

use std::ffi::OsString;
use std::time::Duration;
use log::{info, error, LevelFilter};
use tokio::sync::mpsc;
use windows_service::{
    define_windows_service,
    service::{
        ServiceControl, ServiceControlAccept, ServiceExitCode,
        ServiceState, ServiceStatus, ServiceType,
    },
    service_control_handler::{self, ServiceControlHandlerResult},
    service_dispatcher,
    service_manager::{ServiceManager, ServiceManagerAccess},
    service::{ServiceAccess, ServiceStartType, ServiceErrorControl, ServiceInfo},
};

use camera_manager::{CameraManager, CAMERA_MANAGER};
use plate_service::PlateService;
use config::Config;

const SERVICE_NAME:    &str = "DahuaParkingService";
const SERVICE_DISPLAY: &str = "zevDahuaRust";
const SERVICE_DESC:    &str = "Dahua ALPR camera plate reader with barrier control";

// ─── Windows Service boilerplate ─────────────────────────────────────────────

define_windows_service!(ffi_service_main, service_main);

fn service_main(args: Vec<OsString>) {
    if let Err(e) = run_service(args) {
        error!("Service fatal error: {e}");
    }
}

fn run_service(_args: Vec<OsString>) -> anyhow::Result<()> {
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();

    let event_handler = move |control: ServiceControl| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                let _ = stop_tx.send(());
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, event_handler)?;

    status_handle.set_service_status(ServiceStatus {
        service_type:      ServiceType::OWN_PROCESS,
        current_state:     ServiceState::StartPending,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code:         ServiceExitCode::Win32(0),
        checkpoint:        0,
        wait_hint:         Duration::from_secs(15),
        process_id:        None,
    })?;

    init_logging(LevelFilter::Info);

    let cfg = match Config::load() {
        Ok(c) => c,
        Err(e) => {
            error!("Config load failed: {e}");
            status_handle.set_service_status(ServiceStatus {
                service_type:      ServiceType::OWN_PROCESS,
                current_state:     ServiceState::Stopped,
                controls_accepted: ServiceControlAccept::empty(),
                exit_code:         ServiceExitCode::ServiceSpecific(1),
                checkpoint: 0, wait_hint: Duration::ZERO, process_id: None,
            })?;
            return Ok(());
        }
    };

    status_handle.set_service_status(ServiceStatus {
        service_type:      ServiceType::OWN_PROCESS,
        current_state:     ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code:         ServiceExitCode::Win32(0),
        checkpoint: 0, wait_hint: Duration::ZERO, process_id: None,
    })?;

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async {
        tokio::select! {
            result = run_app(cfg) => {
                if let Err(e) = result { error!("App error: {e}"); }
            }
            _ = tokio::task::spawn_blocking(move || { let _ = stop_rx.recv(); }) => {
                info!("Stop signal received");
            }
        }
    });

    status_handle.set_service_status(ServiceStatus {
        service_type:      ServiceType::OWN_PROCESS,
        current_state:     ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code:         ServiceExitCode::Win32(0),
        checkpoint: 0, wait_hint: Duration::ZERO, process_id: None,
    })?;

    Ok(())
}

// ─── Main application logic ───────────────────────────────────────────────────

async fn run_app(cfg: Config) -> anyhow::Result<()> {
    info!("=== Dahua Parking Service starting ===");

    // Suppress debug error dialogs from DLLs
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
            windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX
        );
    }

    // 1. Plate event channel
    let (plate_tx, mut plate_rx) = mpsc::channel::<camera_manager::PlateEvent>(128);

    // 2. Init CameraManager
    let manager = CameraManager::new(&cfg, plate_tx.clone());
    CAMERA_MANAGER.set(manager)
        .map_err(|_| anyhow::anyhow!("CameraManager already initialized"))?;

    // 3. PlateService
    let plate_svc = std::sync::Arc::new(PlateService::new(cfg.server.clone())?);

    // 4. Connect cameras (blocking)
    tokio::task::spawn_blocking(move || {
    if let Err(e) = CAMERA_MANAGER.get().unwrap().startup_and_connect() {
        error!("SDK startup failed: {e}");
    }
});

    // 5. Start HTTP plate listeners for each camera
    for cam in &cfg.cameras {
        let ip       = cam.ip.clone();
        let password = cam.password.clone();
        let port     = cam.http_port.unwrap_or(80);
        let tx       = plate_tx.clone();
        tokio::spawn(async move {
            plate_listener::run_plate_listener(ip, password, port, tx).await;
        });
    }

    // 6. Heartbeat loop
    let heartbeat_interval = cfg.sdk.heartbeat_interval_secs;
    tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(heartbeat_interval)).await;
            tokio::task::spawn_blocking(|| {
                if let Some(mgr) = CAMERA_MANAGER.get() {
                    mgr.heartbeat();
                }
            });
        }
    });

    // 7. API server
    tokio::spawn(api::run_api_server(5000));

    // 8. Plate event processor
    info!("Plate event processor started");
    loop {
        match plate_rx.recv().await {
            Some(event) => {
                let svc = std::sync::Arc::clone(&plate_svc);
                tokio::spawn(async move {
                    svc.process_plate(&event).await;
                });
            }
            None => {
                println!("Plate channel closed, keeping service alive...");
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                }
            }
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() {
    // Suppress debug error dialogs
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
            windows_sys::Win32::System::Diagnostics::Debug::SEM_NOGPFAULTERRORBOX
        );
    }

    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("install")   => install_service(),
        Some("uninstall") => uninstall_service(),
        Some("run")       => run_interactive(),
        _ => {
            service_dispatcher::start(SERVICE_NAME, ffi_service_main)
                .expect("Service dispatcher failed — run as Windows service or use 'run'");
        }
    }
}

fn run_interactive() {
    init_logging(LevelFilter::Debug);
    println!("Running interactively (not as Windows service)");

    let cfg = Config::load().expect("Cannot load config.toml");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            if let Err(e) = run_app(cfg).await {
                error!("Error: {e}");
            }
        });
}

// ─── Service install / uninstall ─────────────────────────────────────────────

fn install_service() {
    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    ).expect("Open SCM failed — run as Administrator");

    let exe = std::env::current_exe().expect("Cannot get exe path");
    let info = ServiceInfo {
        name:             OsString::from(SERVICE_NAME),
        display_name:     OsString::from(SERVICE_DISPLAY),
        service_type:     ServiceType::OWN_PROCESS,
        start_type:       ServiceStartType::AutoStart,
        error_control:    ServiceErrorControl::Normal,
        executable_path:  exe,
        launch_arguments: vec![],
        dependencies:     vec![],
        account_name:     None,
        account_password: None,
    };

    match manager.create_service(&info, ServiceAccess::CHANGE_CONFIG) {
        Ok(svc) => {
            svc.set_description(SERVICE_DESC).ok();
            println!("✓ Service '{SERVICE_NAME}' installed.");
            println!("  Start: net start {SERVICE_NAME}");
        }
        Err(e) => { eprintln!("✗ Install failed: {e}"); std::process::exit(1); }
    }
}

fn uninstall_service() {
    let manager = ServiceManager::local_computer(
        None::<&str>, ServiceManagerAccess::CONNECT,
    ).expect("Open SCM failed");

    let svc = manager.open_service(
        SERVICE_NAME, ServiceAccess::DELETE | ServiceAccess::STOP,
    ).expect("Service not found");

    let _ = svc.stop();
    std::thread::sleep(Duration::from_secs(2));

    match svc.delete() {
        Ok(_)  => println!("✓ Service '{SERVICE_NAME}' removed."),
        Err(e) => { eprintln!("✗ Uninstall failed: {e}"); std::process::exit(1); }
    }
}

fn init_logging(level: LevelFilter) {
    env_logger::Builder::new().filter_level(level).init();
}
