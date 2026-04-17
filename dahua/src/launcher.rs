//! combined-launcher.exe — spawns and monitors parking-service.exe (ZK, 32-bit)
//! and dahua-service.exe (Dahua, 64-bit), restarting either if it dies.
//!
//! Usage:
//!   combined-launcher.exe install    — install as Windows service
//!   combined-launcher.exe uninstall  — remove Windows service
//!   combined-launcher.exe run        — run interactively (debug)
//!   (no args)                        — called by Windows SCM

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::mpsc;
use std::time::{Duration, Instant};
use log::{info, warn, error, LevelFilter};
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

const SERVICE_NAME:    &str = "CombinedParkingLauncher";
const SERVICE_DISPLAY: &str = "zevCombinedLauncher";
const SERVICE_DESC:    &str = "Launcher that manages parking-service (ZK) and dahua-service (Dahua)";

// ─── Windows Service boilerplate ─────────────────────────────────────────────

define_windows_service!(ffi_service_main, service_main);

fn service_main(args: Vec<OsString>) {
    if let Err(e) = run_service(args) {
        error!("Service fatal error: {e}");
    }
}

fn run_service(_args: Vec<OsString>) -> anyhow::Result<()> {
    let (stop_tx, stop_rx) = mpsc::channel::<()>();

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

    status_handle.set_service_status(ServiceStatus {
        service_type:      ServiceType::OWN_PROCESS,
        current_state:     ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP,
        exit_code:         ServiceExitCode::Win32(0),
        checkpoint:        0,
        wait_hint:         Duration::ZERO,
        process_id:        None,
    })?;

    run_monitor_loop(stop_rx);

    status_handle.set_service_status(ServiceStatus {
        service_type:      ServiceType::OWN_PROCESS,
        current_state:     ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code:         ServiceExitCode::Win32(0),
        checkpoint:        0,
        wait_hint:         Duration::ZERO,
        process_id:        None,
    })?;

    Ok(())
}

// ─── Managed child process ────────────────────────────────────────────────────

struct ManagedProcess {
    name:  String,
    exe:   PathBuf,
    child: Option<Child>,
    last_start: Option<Instant>,
}

impl ManagedProcess {
    fn new(name: &str, exe: PathBuf) -> Self {
        Self { name: name.to_string(), exe, child: None, last_start: None }
    }

    /// Kill stale instances by exe name, then spawn a fresh one with "run".
    fn start(&mut self) {
        // Kill any leftover instances so the new one can bind ports
        if let Some(exe_name) = self.exe.file_name().and_then(|n| n.to_str()) {
            let _ = Command::new("taskkill")
                .args(["/F", "/IM", exe_name])
                .output();
            std::thread::sleep(Duration::from_millis(500));
        }

        match Command::new(&self.exe).arg("run").spawn() {
            Ok(child) => {
                info!("[{}] Started (pid={})", self.name, child.id());
                self.child = Some(child);
                self.last_start = Some(Instant::now());
            }
            Err(e) => {
                error!("[{}] Failed to start: {e}", self.name);
                self.child = None;
            }
        }
    }

    /// Returns true if the child is still running.
    fn is_running(&mut self) -> bool {
        match &mut self.child {
            None => false,
            Some(child) => match child.try_wait() {
                Ok(None)    => true,  // still running
                Ok(Some(s)) => { warn!("[{}] Exited with status {s}", self.name); false }
                Err(e)      => { warn!("[{}] try_wait error: {e}", self.name); false }
            },
        }
    }

    /// Restart if dead, with a 5-second minimum between restarts.
    fn check_and_restart(&mut self) {
        if !self.is_running() {
            // Back off if we started very recently (avoid tight crash loop)
            if let Some(t) = self.last_start {
                if t.elapsed() < Duration::from_secs(5) {
                    return;
                }
            }
            warn!("[{}] Not running — restarting...", self.name);
            self.start();
        }
    }

    fn kill(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            info!("[{}] Killed", self.name);
        }
    }
}

// ─── Monitor loop ─────────────────────────────────────────────────────────────

fn run_monitor_loop(stop_rx: mpsc::Receiver<()>) {
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let mut zk    = ManagedProcess::new("parking-service",  exe_dir.join("parking-service.exe"));
    let mut dahua = ManagedProcess::new("dahua-service",    exe_dir.join("dahua-service.exe"));

    info!("Launcher: starting both services...");
    zk.start();
    // Small delay so ZK binds port 5000 first (Dahua API is on 5001)
    std::thread::sleep(Duration::from_secs(2));
    dahua.start();

    loop {
        // Only stop on an explicit stop signal (Ok(())).
        // Disconnected means the channel was dropped — that should NOT happen
        // because ctrlc_or_stdin / the service handler keeps stop_tx alive.
        if let Ok(()) = stop_rx.try_recv() {
            info!("Launcher: stop дохио — хоёр процессыг зогсооно");
            dahua.kill();
            zk.kill();
            return;
        }

        zk.check_and_restart();
        dahua.check_and_restart();

        std::thread::sleep(Duration::from_secs(5));
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

fn main() {
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
    init_logging(LevelFilter::Info);
    println!("Running interactively (not as Windows service). Press Ctrl+C to stop.");

    let (stop_tx, stop_rx) = mpsc::channel::<()>();

    // Keep stop_tx alive in a background thread so the monitor loop never sees
    // Disconnected — it only exits on Ok(()) which arrives from the Ctrl+C handler.
    ctrlc_or_stdin(stop_tx);

    run_monitor_loop(stop_rx);
}

/// Sets up Ctrl+C handler and keeps `stop_tx` alive in a background thread.
/// The Ctrl+C handler sends Ok(()) to trigger a clean shutdown.
fn ctrlc_or_stdin(stop_tx: mpsc::Sender<()>) {
    // Clone for the Ctrl+C handler closure
    let tx_ctrlc = stop_tx.clone();
    ctrlc::set_handler(move || {
        println!("\nCtrl+C — зогсоож байна...");
        let _ = tx_ctrlc.send(());
    }).ok();

    // Keep the original stop_tx alive forever so the monitor loop's
    // try_recv() never returns Disconnected.
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(Duration::from_secs(3600));
            let _ = &stop_tx; // prevent drop
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

            // Auto-restart on failure: 5s / 10s / 30s
            let _ = std::process::Command::new("sc")
                .args([
                    "failure", SERVICE_NAME,
                    "reset=", "60",
                    "actions=", "restart/5000/restart/10000/restart/30000",
                ])
                .output();

            println!("✓ Service '{SERVICE_NAME}' installed.");
            println!("  Auto-restart on failure: 5s / 10s / 30s");
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
    let exe_dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let log_path = exe_dir.join("launcher.log");

    fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "[{}] [{}] {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                record.level(),
                message
            ))
        })
        .level(level)
        .chain(std::io::stdout())
        .chain(fern::log_file(&log_path).expect("Cannot open launcher.log"))
        .apply()
        .expect("Logger init failed");

    log::info!("Log file: {}", log_path.display());
}
