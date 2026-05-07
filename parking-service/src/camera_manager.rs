use std::collections::{HashMap, HashSet};
use std::ffi::{c_void, CString};
use std::ptr;
use std::sync::{Arc, Mutex};
use log::{warn, error};
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use crate::config::{CameraEntry, Config, SdkConfig};
use crate::sdk::{AlprSdk, DevInfo, E_CLIENT_NORMAL, LPR_DEV_GZ, read_wide_str};
use crate::callbacks::callback_for_handle;

pub static CAMERA_MANAGER: OnceCell<CameraManager> = OnceCell::new();

#[derive(Debug, Clone)]
pub struct PlateEvent {
    pub plate:     String,
    pub camera_ip: String,
    pub handle:    i32,
}

#[derive(Clone)]
struct CameraState {
    handle: i32,
    ip:     String,
    port:   u16,
    pwd:    String,
}

pub struct CameraManager {
    cameras:       Arc<Mutex<Vec<CameraState>>>,
    handle_ip:     Arc<Mutex<HashMap<i32, String>>>,
    reconnecting:  Arc<Mutex<HashSet<i32>>>,
    pending_gates: Arc<Mutex<HashSet<String>>>,
    sdk_cfg:       SdkConfig,
    cam_cfg:       Vec<CameraEntry>,
    username:      String,
    pub plate_tx:  mpsc::Sender<PlateEvent>,
    pub gate_tx:   Arc<Mutex<std::sync::mpsc::SyncSender<i32>>>,
}

impl CameraManager {
    pub fn new(cfg: &Config, plate_tx: mpsc::Sender<PlateEvent>) -> Self {
    let (initial_tx, initial_rx) = std::sync::mpsc::sync_channel::<i32>(8);
    let gate_tx_arc = Arc::new(Mutex::new(initial_tx));
    let supervisor_arc = Arc::clone(&gate_tx_arc);

    // Supervisor thread: restarts the gate worker if it ever dies
    std::thread::spawn(move || {
        let mut current_rx = Some(initial_rx);
        loop {
            let rx = current_rx.take().unwrap();
            let worker = std::thread::spawn(move || {
                while let Ok(handle) = rx.recv() {
                    if let Ok(sdk) = AlprSdk::load() {
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            let ret = unsafe { (sdk.open_gate)(handle) };
                            println!("Khaalga ongoilgohoos irj bui khariu: {ret}");
                        }));
                        if result.is_err() {
                            error!("Gate open panicked for handle {handle}");
                        }
                    }
                }
            });
            let _ = worker.join();
            error!("Gate thread died — respawning in 500ms");
            std::thread::sleep(std::time::Duration::from_millis(500));
            let (new_tx, new_rx) = std::sync::mpsc::sync_channel::<i32>(8);
            *supervisor_arc.lock().unwrap() = new_tx;
            current_rx = Some(new_rx);
        }
    });

    Self {
        cameras:       Arc::new(Mutex::new(Vec::new())),
        handle_ip:     Arc::new(Mutex::new(HashMap::new())),
        reconnecting:  Arc::new(Mutex::new(HashSet::new())),
        pending_gates: Arc::new(Mutex::new(HashSet::new())),
        sdk_cfg:       cfg.sdk.clone(),
        cam_cfg:       cfg.cameras.clone(),
        username:      cfg.sdk.username.clone(),
        plate_tx,
        gate_tx: gate_tx_arc,
    }
}

    pub fn ip_for_handle(&self, handle: i32) -> Option<String> {
        self.handle_ip.lock().ok()?.get(&handle).cloned()
    }

    pub fn camera_type_for_ip(&self, ip: &str) -> &str {
        self.cam_cfg
            .iter()
            .find(|c| c.ip == ip)
            .map(|c| c.camera_type.as_str())
            .unwrap_or("zk")
    }

    pub fn password_for_ip(&self, ip: &str) -> Option<String> {
        self.cam_cfg.iter().find(|c| c.ip == ip).map(|c| c.password.clone())
    }

    pub fn gate_for_ip(&self, ip: &str) -> &str {
        self.cam_cfg.iter().find(|c| c.ip == ip).map(|c| c.gate.as_str()).unwrap_or("")
    }

    pub fn org_name(&self) -> &str {
        &self.sdk_cfg.org_name
    }

    pub fn company_name(&self) -> &str {
        &self.sdk_cfg.company_name
    }

    pub fn handle_for_ip(&self, ip: &str) -> Option<i32> {
        let map = self.handle_ip.lock().ok()?;
        map.iter().find(|(_, v)| v.as_str() == ip).map(|(k, _)| *k)
    }

    pub fn camera_count(&self) -> usize {
        self.handle_ip.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn startup_and_connect(&self) -> anyhow::Result<()> {
        let sdk = AlprSdk::load()?;

        println!("Kholbolt func ruu orloo");
        let ret = unsafe { (sdk.startup)(ptr::null_mut(), 0x500) };
        if ret != 0 {
            anyhow::bail!("AlprSDK_Startup failed: {ret}");
        }
        println!("Amjilttai aslaa: {ret}");
        // Camera connections are handled by the heartbeat loop
        Ok(())
    }

    pub fn connect_all(&self) {
        let sdk = match AlprSdk::load() { Ok(s) => s, Err(e) => { error!("{e}"); return; } };

        // Mark all cameras as reconnecting so open_gate queues instead of dropping
        {
            let mut r = self.reconnecting.lock().unwrap();
            for i in 0..self.cam_cfg.len() { r.insert(i as i32); }
        }

        let mut map  = self.handle_ip.lock().unwrap();
        let mut cams = self.cameras.lock().unwrap();
        map.clear();
        cams.clear();

        for (i, cam) in self.cam_cfg.iter().enumerate() {
            let handle = i as i32;

            if cam.camera_type == "dahua" {
                println!("Skipping Dahua camera {} — handled by Dahua service", cam.ip);
                self.reconnecting.lock().unwrap().remove(&handle);
                continue;
            }

            unsafe {
                let _ = (sdk.clear_recog_task)(handle);
                let _ = (sdk.disconnect_dev)(handle);
                let _ = (sdk.uninit_handle)(handle);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));

            let init_ret = unsafe { (sdk.init_handle)(handle, ptr::null_mut()) };
            if init_ret < 0 {
                error!("AlprSDK_InitHandle failed for {} (handle {handle}): {init_ret}", cam.ip);
                continue;
            }

            let dev = DevInfo::new(&cam.ip, cam.port, &self.username, &cam.password, LPR_DEV_GZ);
            let connected = self.connect_with_retry(sdk, handle, &dev, &cam.ip);

            if connected {
                map.insert(handle, cam.ip.clone());
                cams.push(CameraState {
                    handle,
                    ip:  cam.ip.clone(),
                    port: cam.port,
                    pwd: cam.password.clone(),
                });

                unsafe { let _ = (sdk.start_video)(handle); }

                if let Some(cb) = callback_for_handle(i) {
                    let cb_ret = unsafe { (sdk.create_recog_task)(handle, cb, ptr::null_mut()) };
                    if cb_ret >= 0 {
                        println!("Callback amjilttai uuslee: {cb_ret} ({})", cam.ip);
                    } else {
                        error!("CreateRecogAllInfoTask failed for {}: {cb_ret}", cam.ip);
                    }
                } else {
                    warn!("No callback slot for handle {handle} — max 8 cameras");
                }
            } else {
                error!("Failed to connect {} after all retries", cam.ip);
            }

            // Unmark reconnecting for this camera
            self.reconnecting.lock().unwrap().remove(&handle);

            println!("Burtgegdsen Camera count: {}", cams.len());
        }

        // Fire any gate opens that were queued during reconnection
        let pending: Vec<String> = self.pending_gates.lock().unwrap().drain().collect();
        drop(map); drop(cams);
        for ip in pending {
            println!("Pending gate open firing for {ip} after connect_all");
            self.open_gate(&ip);
        }
    }

    fn connect_with_retry(&self, sdk: &AlprSdk, handle: i32, dev: &DevInfo, ip: &str) -> bool {
        let max     = self.sdk_cfg.max_connect_retries as usize;
        let base_ms = self.sdk_cfg.connect_timeout_ms as i32;

        for attempt in 1..=max {
            let timeout = base_ms + (attempt as i32 * 500);
            unsafe { let _ = (sdk.set_connect_timeout)(handle, timeout); }

            println!("Connecting to camera {ip} (handle {handle}) — attempt {attempt}/{max}");
            let login_id = unsafe { (sdk.connect_dev)(handle, dev as *const DevInfo, E_CLIENT_NORMAL) };

            if login_id >= 0 {
                println!("Successfully connected to camera {ip} on attempt {attempt}");
                return true;
            }

            warn!("Connection attempt {attempt} failed for {ip}: {login_id}");

            if attempt < max {
                let delay_ms = 1000u64 * attempt as u64;
                println!("Waiting {delay_ms}ms before retry {}...", attempt + 1);
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));

                unsafe {
                    let _ = (sdk.disconnect_dev)(handle);
                    let _ = (sdk.uninit_handle)(handle);
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
                let reinit = unsafe { (sdk.init_handle)(handle, ptr::null_mut()) };
                if reinit < 0 {
                    error!("Re-init handle {handle} failed — aborting retries for {ip}");
                    break;
                }
            }
        }
        false
    }

    pub fn open_gate(&self, ip: &str) -> bool {
        if let Some(handle) = self.handle_for_ip(ip) {
            if self.reconnecting.lock().unwrap().contains(&handle) {
                warn!("open_gate: camera {ip} is reconnecting, queuing gate open");
                self.pending_gates.lock().unwrap().insert(ip.to_string());
                return true;
            }
            println!("Khaalga ongoilgohoos irj bui khariu: ({ip})");
            if self.gate_tx.lock().unwrap().send(handle).is_err() {
                error!("Gate channel send failed for {ip}");
                return false;
            }
            return true;
        }
        // handle_ip has no entry — check if this IP is currently reconnecting
        let reconnecting_handle = self.cam_cfg.iter().enumerate().find_map(|(i, cam)| {
            if cam.ip == ip { Some(i as i32) } else { None }
        });
        if let Some(handle) = reconnecting_handle {
            if self.reconnecting.lock().unwrap().contains(&handle) {
                warn!("open_gate: camera {ip} is reconnecting, queuing gate open");
                self.pending_gates.lock().unwrap().insert(ip.to_string());
                return true;
            }
        }
        warn!("open_gate: IP {ip} not in handle list");
        false
    }

    pub fn heartbeat(&self) {
        let sdk = match AlprSdk::load() { Ok(s) => s, Err(e) => { error!("{e}"); return; } };

        for (i, cam) in self.cam_cfg.iter().enumerate() {
            let handle = i as i32;

            if cam.camera_type == "dahua" {
                continue;
            }

            let connected = self.handle_ip.lock().unwrap().contains_key(&handle);

            // Skip cameras already being reconnected
            if self.reconnecting.lock().unwrap().contains(&handle) {
                println!("Camera {} (handle {handle}) reconnecting, skipping heartbeat", cam.ip);
                continue;
            }

            if connected {
                let ret = unsafe { (sdk.send_heartbeat)(handle) };
                if ret != 0 {
                    println!("xolbolt baisangui {ret} ({})", cam.ip);
                    unsafe {
                        let _ = (sdk.disconnect_dev)(handle);
                        let _ = (sdk.uninit_handle)(handle);
                        let _ = (sdk.clear_recog_task)(handle);
                    }
                    self.handle_ip.lock().unwrap().remove(&handle);
                    self.reconnecting.lock().unwrap().insert(handle);

                    let ip            = cam.ip.clone();
                    let pwd           = cam.password.clone();
                    let port          = cam.port;
                    let user          = self.username.clone();
                    let sdk_cfg       = self.sdk_cfg.clone();
                    let ip_for_error  = ip.clone();
                    let reconnecting  = Arc::clone(&self.reconnecting);
                    let pending_gates = Arc::clone(&self.pending_gates);
                    std::thread::Builder::new()
                        .name(format!("reconnect-{ip}"))
                        .spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            let _ = std::panic::catch_unwind(|| {
                                reconnect_camera(handle, &ip, port, &user, &pwd, &sdk_cfg);
                            });
                            reconnecting.lock().unwrap().remove(&handle);
                            if pending_gates.lock().unwrap().remove(&ip) {
                                if let Some(mgr) = CAMERA_MANAGER.get() {
                                    println!("Pending gate open firing for {ip} after reconnect");
                                    mgr.open_gate(&ip);
                                }
                            }
                        })
                        .unwrap_or_else(|e| {
                            error!("Failed to spawn reconnect thread for {ip_for_error}: {e}");
                            std::thread::spawn(|| {})
                        });
                } else {
                    println!("xolbolt bainaa 0 ({})", cam.ip);
                }
            } else {
                // Not connected yet — try to connect via heartbeat
                println!("Camera {} (handle {handle}) not connected, connecting...", cam.ip);
                self.reconnecting.lock().unwrap().insert(handle);

                let ip            = cam.ip.clone();
                let pwd           = cam.password.clone();
                let port          = cam.port;
                let user          = self.username.clone();
                let sdk_cfg       = self.sdk_cfg.clone();
                let ip_for_error  = ip.clone();
                let reconnecting  = Arc::clone(&self.reconnecting);
                let pending_gates = Arc::clone(&self.pending_gates);
                std::thread::Builder::new()
                    .name(format!("connect-{ip}"))
                    .spawn(move || {
                        let _ = std::panic::catch_unwind(|| {
                            reconnect_camera(handle, &ip, port, &user, &pwd, &sdk_cfg);
                        });
                        reconnecting.lock().unwrap().remove(&handle);
                        if pending_gates.lock().unwrap().remove(&ip) {
                            if let Some(mgr) = CAMERA_MANAGER.get() {
                                println!("Pending gate open firing for {ip} after connect");
                                mgr.open_gate(&ip);
                            }
                        }
                    })
                    .unwrap_or_else(|e| {
                        error!("Failed to spawn connect thread for {ip_for_error}: {e}");
                        std::thread::spawn(|| {})
                    });
            }
        }
    }

    pub fn display_on_screen(&self, ip: &str, text: &str, dun: &str) -> bool {
        // ── Change this text to whatever you want on the first line of the screen ──
        const DISPLAY_LINE1: &str = "ParkEase";
        // ────────────────────────────────────────────────────────────────────────────

        let sdk = match AlprSdk::load() { Ok(s) => s, Err(_) => return false };
        let handle = match self.handle_for_ip(ip) { Some(h) => h, None => {
            println!("!!! display_on_screen: IP {ip} камерын жагсаалтад байхгүй");
            return false;
        }};

        // Determine gate role from config ("entrance" shows header+plate+amount;
        // "exit" or anything else shows plate+amount only).
        let is_entrance = self.cam_cfg
            .iter()
            .find(|c| c.ip == ip)
            .map(|c| c.gate.to_lowercase() == "entrance")
            .unwrap_or(false);

        println!(">>> Дэлгэц дээр хэвлэж байна | IP: {ip} handle: {handle} gate: {}",
            if is_entrance { "entrance" } else { "exit" });
        if is_entrance {
            println!("    normal[0] : {DISPLAY_LINE1}");
            println!("    normal[1] : {text}");
        } else {
            println!("    normal[0] : {text}");
            println!("    normal[1] : {dun}");
        }

        // SDK expects null-terminated C strings. CString guarantees the \0 at the end.
        // Raw .as_bytes() has NO null terminator → SDK reads past the buffer → crash!
        let safe = |s: &str| std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default();
        // Entrance gate: line1=header, line2=plate, line3=amount
        // Exit gate:     line1=plate,  line2=amount, line3=empty
        let c1 = safe(if is_entrance { DISPLAY_LINE1 } else { text });
        let c2 = safe(if is_entrance { text } else { dun });
        let c3 = safe("");
        let c4 = safe("");

        let ret = unsafe {
            (sdk.trans2screen)(
                handle, 0,
                1, c1.as_ptr() as *const u8, // мөр 1 = тогтмол текст
                1, c2.as_ptr() as *const u8, // мөр 2 = дугаар
                1, c3.as_ptr() as *const u8, // мөр 3 = дүн
                1, c4.as_ptr() as *const u8, // мөр 4 = хоосон
            )
        };

        if ret >= 0 {
            println!("<<< Дэлгэц амжилттай хэвлэгдлээ | IP: {ip} (ret={ret})");
        } else {
            println!("!!! Дэлгэц хэвлэж чадсангүй | IP: {ip} (ret={ret})");
        }
        ret >= 0
    }
    pub fn display_on_screen_ognootoi(&self, ip: &str, text: &str, dun: &str, start: &str, end: &str) -> bool {
    let sdk = match AlprSdk::load() { Ok(s) => s, Err(_) => return false };
    let handle = match self.handle_for_ip(ip) {
        Some(h) => h,
        None => {
            println!("display_on_screen_ognootoi: IP {ip} олдсонгүй");
            return false;
        }
    };

    let safe = |s: &str| std::ffi::CString::new(s.replace('\0', "")).unwrap_or_default();

    let c1 = safe(text);   // мөр 1 = plate
    let c2 = safe(dun);    // мөр 2 = дүн
    let c3 = safe(start);  // мөр 3 = эхлэх огноо
    let c4 = safe(end);    // мөр 4 = дуусах огноо

    println!(">>> sambarOgnootoi | IP: {ip} text={text} dun={dun} start={start} end={end}");

    let ret = unsafe {
        (sdk.trans2screen)(
            handle, 0,
            1, c1.as_ptr() as *const u8,
            1, c2.as_ptr() as *const u8,
            1, c3.as_ptr() as *const u8,
            1, c4.as_ptr() as *const u8,
        )
    };

    println!("<<< sambarOgnootoi: {} ip={ip}", if ret >= 0 { "OK" } else { "FAIL" });
    ret >= 0
}

}

fn reconnect_camera(handle: i32, ip: &str, port: u16, username: &str, pwd: &str, cfg: &SdkConfig) {
    let sdk = match AlprSdk::load() { Ok(s) => s, Err(e) => { error!("{e}"); return; } };

    println!("Reconnecting camera {ip} (handle: {handle})...");

    unsafe {
        let _ = (sdk.clear_recog_task)(handle);
        let _ = (sdk.disconnect_dev)(handle);
        let _ = (sdk.uninit_handle)(handle);
    }
    std::thread::sleep(std::time::Duration::from_millis(100));

    let init_ret = unsafe { (sdk.init_handle)(handle, ptr::null_mut()) };
    if init_ret < 0 {
        error!("Re-init handle {handle} failed for {ip}: {init_ret}");
        return;
    }

    let dev  = DevInfo::new(ip, port, username, pwd, LPR_DEV_GZ);
    let max  = cfg.max_connect_retries as usize;
    let base = cfg.connect_timeout_ms as i32;

    for attempt in 1..=max {
        let timeout = base + (attempt as i32 * 500);
        unsafe { let _ = (sdk.set_connect_timeout)(handle, timeout); }

        let login_id = unsafe { (sdk.connect_dev)(handle, &dev as *const DevInfo, E_CLIENT_NORMAL) };
        println!("  reconnect attempt {attempt}/{max} → login_id={login_id} (0x{login_id:08X})");

        if login_id >= 0 {
            println!("Reconnected {ip} on attempt {attempt}");

            unsafe { let _ = (sdk.start_video)(handle); }

            if let Some(cb) = callback_for_handle(handle as usize) {
                let cb_ret = unsafe { (sdk.create_recog_task)(handle, cb, ptr::null_mut()) };
                if cb_ret >= 0 {
                    println!("Callback amjilttai uuslee (reconnect): {cb_ret}");
                } else {
                    error!("CreateRecogTask failed after reconnect for {ip}: {cb_ret}");
                }
            }

            if let Some(mgr) = CAMERA_MANAGER.get() {
                mgr.handle_ip.lock().unwrap().insert(handle, ip.to_string());
            }
            return;
        }

        if attempt < max {
            std::thread::sleep(std::time::Duration::from_millis(1000 * attempt as u64));
            unsafe {
                let _ = (sdk.disconnect_dev)(handle);
                let _ = (sdk.uninit_handle)(handle);
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            unsafe { let _ = (sdk.init_handle)(handle, ptr::null_mut()); }
        }
    }

    error!("Failed to reconnect {ip} after {max} retries");
}

pub unsafe extern "system" fn server_find_callback(
    _n_device_type: std::ffi::c_int,
    _p_name:        *const u16,
    p_ip:           *const u16,
    _mac:           *mut c_void,
    _port_web:      crate::sdk::c_ushort,
    _port_listen:   crate::sdk::c_ushort,
    _submask:       *const u16,
    _gateway:       *const u16,
    _multi:         *const u16,
    _dns:           *const u16,
    _multi_port:    crate::sdk::c_ushort,
    _channels:      std::ffi::c_int,
    n_find_count:   std::ffi::c_int,
    _device_id:     std::ffi::c_int,
) {
    println!("Server Find callback ajillaa");
    println!("oldson niit camera: {n_find_count}");
    if let Some(ip) = read_wide_str(p_ip) {
        println!("oldson ip: {ip}");
    }
}
