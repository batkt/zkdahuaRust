use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ffi::{c_void, CString};
use log::{warn, error};
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use crate::config::{CameraEntry, Config, SdkConfig};
use crate::sdk::{
    DahuaSdk, NET_IN_LOGIN_WITH_HIGHLEVEL_SECURITY, NET_OUT_LOGIN_WITH_HIGHLEVEL_SECURITY,
    NET_CTRL_OPEN_STROBE, NET_CFG_TRAFFIC_LATTICE_SCREEN_INFO,
    EM_CTRL_OPEN_STROBE, EM_SCREEN_SHOW_CONTENTS_CUSTOM,
    EM_SCREEN_SHOW_CONTENTS_INTIME, EM_SCREEN_SHOW_CONTENTS_OUTTIME,
    EM_LATTICE_SCREEN_SHOW_TYPE_WORD_CONTROL, EM_LATTICE_SCREEN_CONTROL_TYPE_CAMERA_CONTROL,
    fill_ansi, HANDLE,
};

pub static CAMERA_MANAGER: OnceCell<CameraManager> = OnceCell::new();

#[derive(Debug, Clone)]
pub struct PlateEvent {
    pub plate:     String,
    pub camera_ip: String
}

#[derive(Clone)]
struct CameraState {
    handle:   HANDLE,
    ip:       String,
    password: String,
}

unsafe impl Send for CameraState {}
unsafe impl Sync for CameraState {}

pub struct CameraManager {
    handle_map: Arc<Mutex<HashMap<String, HANDLE>>>,
    cameras:    Arc<Mutex<Vec<CameraState>>>,
    sdk_cfg:    SdkConfig,
    cam_cfg:    Vec<CameraEntry>,
    pub plate_tx: mpsc::Sender<PlateEvent>,
}

unsafe impl Send for CameraManager {}
unsafe impl Sync for CameraManager {}

impl CameraManager {
    pub fn new(cfg: &Config, plate_tx: mpsc::Sender<PlateEvent>) -> Self {
        Self {
            handle_map: Arc::new(Mutex::new(HashMap::new())),
            cameras:    Arc::new(Mutex::new(Vec::new())),
            sdk_cfg:    cfg.sdk.clone(),
            cam_cfg:    cfg.cameras.clone(),
            plate_tx,
        }
    }

    pub fn handle_for_ip(&self, ip: &str) -> Option<HANDLE> {
        self.handle_map.lock().ok()?.get(ip).copied()
    }

    pub fn camera_count(&self) -> usize {
        self.handle_map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn is_entrance(&self, ip: &str) -> bool {
        self.cam_cfg
            .iter()
            .find(|c| c.ip == ip)
            .map(|c| c.gate.as_deref().unwrap_or("").to_lowercase() == "entrance")
            .unwrap_or(false)
    }

    pub fn org_name(&self) -> &str {
        &self.sdk_cfg.org_name
    }

    pub fn company_name(&self) -> &str {
        &self.sdk_cfg.company_name
    }

    pub fn password_for_ip(&self, ip: &str) -> String {
        self.cam_cfg
            .iter()
            .find(|c| c.ip == ip)
            .map(|c| c.password.clone())
            .unwrap_or_else(|| "admin123".to_string())
    }

    pub fn startup_and_connect(&self) -> anyhow::Result<()> {
        let sdk = DahuaSdk::load()?;

        println!("Dahua SDK initialize хийж байна...");
        let ret = unsafe { (sdk.init_ex)(None, std::ptr::null_mut(), std::ptr::null_mut()) };
        if ret == 0 {
            anyhow::bail!("CLIENT_InitEx failed");
        }
        println!("Dahua SDK амжилттай аслаа");

        unsafe {
            (sdk.set_connect_time)(
                self.sdk_cfg.connect_timeout_ms as i32,
                self.sdk_cfg.max_connect_retries as i32,
            );
        }

        self.connect_all();
        Ok(())
    }

    pub fn connect_all(&self) {
        let sdk = match DahuaSdk::load() { Ok(s) => s, Err(e) => { error!("{e}"); return; } };

        let mut map  = self.handle_map.lock().unwrap();
        let mut cams = self.cameras.lock().unwrap();
        map.clear();
        cams.clear();

        for cam in &self.cam_cfg {
            println!("Camera холбож байна: {} ...", cam.ip);

            let handle = Self::connect_with_retry_inner(sdk, &cam.ip, &cam.password, &self.sdk_cfg);
            if handle.is_null() {
                error!("Camera холбогдсонгүй: {}", cam.ip);
                continue;
            }

            println!("Camera амжилттай холбогдлоо: {}", cam.ip);
            map.insert(cam.ip.clone(), handle);
            cams.push(CameraState {
                handle,
                ip:       cam.ip.clone(),
                password: cam.password.clone(),
            });
            println!("Бүртгэгдсэн Camera count: {}", cams.len());
        }
    }

    fn connect_with_retry_inner(sdk: &DahuaSdk, ip: &str, password: &str, sdk_cfg: &SdkConfig) -> HANDLE {
        let max  = sdk_cfg.max_connect_retries as usize;
        let port = sdk_cfg.port as i32;

        for attempt in 1..=max {
            println!("Холболт оролдлого {attempt}/{max} — {ip}");

            let mut in_param  = NET_IN_LOGIN_WITH_HIGHLEVEL_SECURITY::default();
            let mut out_param = NET_OUT_LOGIN_WITH_HIGHLEVEL_SECURITY::default();

            fill_ansi(&mut in_param.szIP,       ip);
            fill_ansi(&mut in_param.szUserName, &sdk_cfg.username);
            fill_ansi(&mut in_param.szPassword, password);
            in_param.nPort    = port;
            in_param.emSpecCap = 0; // TCP

            let handle = unsafe {
                (sdk.login_ex2)(&in_param, &mut out_param, sdk_cfg.connect_timeout_ms as i32)
            };

            if !handle.is_null() {
                println!("Амжилттай холбогдлоо: {ip} (attempt {attempt})");
                return handle;
            }

            let err = unsafe { (sdk.get_last_error)() };
            warn!("Холболт амжилтгүй {ip} attempt {attempt}: error {err:#x}");

            if attempt < max {
                let delay_ms = 1000u64 * attempt as u64;
                println!("{}ms хүлээж байна...", delay_ms);
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            }
        }
        std::ptr::null_mut()
    }

    pub fn open_gate(&self, ip: &str) -> bool {
    let sdk = match DahuaSdk::load() { Ok(s) => s, Err(_) => return false };
    let handle = match self.handle_for_ip(ip) {
        Some(h) => h,
        None => { warn!("open_gate: IP {ip} handle олдсонгүй"); return false; }
    };

    let mut strobe = NET_CTRL_OPEN_STROBE::default();
    strobe.nChannelId = 0;

    println!("NET_CTRL_OPEN_STROBE size: {}", std::mem::size_of::<NET_CTRL_OPEN_STROBE>());
    println!("dwSize field: {}", strobe.dwSize);

    let ret = unsafe {
        (sdk.control_device)(
            handle,
            EM_CTRL_OPEN_STROBE,
            &mut strobe as *mut NET_CTRL_OPEN_STROBE as *mut c_void,
            5000,
        )
    };
    let err = unsafe { (sdk.get_last_error)() };
    println!("Хаалга: {} ({ip}) err={err:#x}", if ret != 0 { "OK" } else { "FAIL" });
    ret != 0
}

    pub fn sambar_type_for_ip(&self, ip: &str) -> String {
    self.cam_cfg
        .iter()
        .find(|c| c.ip == ip)
        .and_then(|c| c.sambar_type.clone())
        .unwrap_or_else(|| "sambar".to_string())
}
    pub fn heartbeat(&self) {
    let sdk = match DahuaSdk::load() { Ok(s) => s, Err(_) => return };
    let cams = self.cameras.lock().unwrap().clone();
    for cam in &cams {
        if cam.handle.is_null() {
            println!("Холбоот байсангүй — дахин холбож байна ({})", cam.ip);
            self.connect_all();
            return;
        }
        println!("Холбоот байна ({})", cam.ip);
    }
}

    pub fn reconnect_all(&self) {
        println!("Бүх камерыг дахин холбож байна...");
        self.connect_all();
    }
}
