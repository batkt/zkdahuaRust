//! Dahua camera manager — SDK login/logout + gate open via CLIENT_ControlDevice

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::ffi::c_void;
use log::{warn, error};
use once_cell::sync::OnceCell;

use crate::config::SdkConfig;
use crate::dahua_sdk::{
    DahuaSdk, NET_IN_LOGIN_WITH_HIGHLEVEL_SECURITY, NET_OUT_LOGIN_WITH_HIGHLEVEL_SECURITY,
    NET_CTRL_OPEN_STROBE, EM_CTRL_OPEN_STROBE, fill_ansi, HANDLE,
};

pub static DAHUA_MANAGER: OnceCell<DahuaCameraManager> = OnceCell::new();

struct CameraState {
    handle:   HANDLE,
    ip:       String,
    password: String,
}

unsafe impl Send for CameraState {}
unsafe impl Sync for CameraState {}

pub struct DahuaCameraManager {
    handle_map:     Arc<Mutex<HashMap<String, HANDLE>>>,
    cameras:        Arc<Mutex<Vec<CameraState>>>,
    reconnect_lock: Arc<Mutex<()>>,
    sdk_cfg:        SdkConfig,
}

unsafe impl Send for DahuaCameraManager {}
unsafe impl Sync for DahuaCameraManager {}

impl DahuaCameraManager {
    pub fn new(sdk_cfg: SdkConfig) -> Self {
        Self {
            handle_map:     Arc::new(Mutex::new(HashMap::new())),
            cameras:        Arc::new(Mutex::new(Vec::new())),
            reconnect_lock: Arc::new(Mutex::new(())),
            sdk_cfg,
        }
    }

    pub fn handle_for_ip(&self, ip: &str) -> Option<HANDLE> {
        self.handle_map.lock().ok()?.get(ip).copied()
    }

    pub fn camera_count(&self) -> usize {
        self.handle_map.lock().map(|m| m.len()).unwrap_or(0)
    }

    pub fn startup(&self) -> anyhow::Result<()> {
        let sdk = DahuaSdk::load()?;
        log::info!("Dahua SDK | initialize хийж байна...");
        let ret = unsafe { (sdk.init_ex)(None, std::ptr::null_mut(), std::ptr::null_mut()) };
        if ret == 0 {
            anyhow::bail!("CLIENT_InitEx failed");
        }
        unsafe {
            (sdk.set_connect_time)(
                self.sdk_cfg.connect_timeout_ms as i32,
                self.sdk_cfg.max_connect_retries as i32,
            );
        }
        log::info!("Dahua SDK | амжилттай аслаа");
        Ok(())
    }

    /// Connect a list of Dahua camera IPs.
    pub fn connect_cameras(&self, cameras: &[(String, String)]) {
        // cameras: [(ip, password)]
        let sdk = match DahuaSdk::load() { Ok(s) => s, Err(e) => { error!("{e}"); return; } };

        let mut map  = self.handle_map.lock().unwrap();
        let mut cams = self.cameras.lock().unwrap();
        map.clear();
        cams.clear();

        for (ip, password) in cameras {
            log::info!("Dahua | camera холбож байна: {ip}");
            let handle = Self::connect_with_retry(sdk, ip, password, &self.sdk_cfg);
            if handle.is_null() {
                error!("Dahua | camera холбогдсонгүй: {ip}");
                continue;
            }
            log::info!("Dahua | camera амжилттай холбогдлоо: {ip}");
            map.insert(ip.clone(), handle);
            cams.push(CameraState { handle, ip: ip.clone(), password: password.clone() });
        }
    }

    fn connect_with_retry(sdk: &DahuaSdk, ip: &str, password: &str, cfg: &SdkConfig) -> HANDLE {
        let max  = cfg.max_connect_retries as usize;
        let port = cfg.dahua_sdk_port as u16;

        for attempt in 1..=max {
            println!("Dahua холболт {attempt}/{max} — {ip}");

            // ── Try CLIENT_LoginEx2 first (most compatible) ───────────────
            let ip_cstr  = std::ffi::CString::new(ip).unwrap_or_default();
            let usr_cstr = std::ffi::CString::new(cfg.username.as_str()).unwrap_or_default();
            let pwd_cstr = std::ffi::CString::new(password).unwrap_or_default();
            let mut dev_info = crate::dahua_sdk::NET_DEVICEINFO_Ex::default();
            let mut err_code: std::ffi::c_int = 0;

            let handle = unsafe {
                (sdk.login_ex2)(
                    ip_cstr.as_ptr(),
                    port,
                    usr_cstr.as_ptr(),
                    pwd_cstr.as_ptr(),
                    0,                          // emSpecCap = TCP
                    std::ptr::null_mut(),
                    &mut dev_info,
                    &mut err_code,
                )
            };

            if !handle.is_null() {
                println!("Dahua LoginEx2 амжилттай: {ip} (attempt {attempt})");
                return handle;
            }
            let sdk_err = unsafe { (sdk.get_last_error)() };
            warn!("Dahua LoginEx2 амжилтгүй {ip} attempt {attempt}: err={sdk_err:#x} code={err_code:#x}");

            // ── Fallback: CLIENT_LoginWithHighLevelSecurity ───────────────
            let mut in_param  = NET_IN_LOGIN_WITH_HIGHLEVEL_SECURITY::default();
            let mut out_param = NET_OUT_LOGIN_WITH_HIGHLEVEL_SECURITY::default();
            fill_ansi(&mut in_param.szIP,       ip);
            fill_ansi(&mut in_param.szUserName, &cfg.username);
            fill_ansi(&mut in_param.szPassword, password);
            in_param.nPort     = port as i32;
            in_param.emSpecCap = 0;

            let handle2 = unsafe {
                (sdk.login_highlevel)(&in_param, &mut out_param, cfg.connect_timeout_ms as i32)
            };

            if !handle2.is_null() {
                println!("Dahua HighLevel login амжилттай: {ip} (attempt {attempt})");
                return handle2;
            }
            let sdk_err2 = unsafe { (sdk.get_last_error)() };
            warn!("Dahua HighLevel login амжилтгүй {ip} attempt {attempt}: err={sdk_err2:#x}");

            if attempt < max {
                std::thread::sleep(std::time::Duration::from_millis(1000 * attempt as u64));
            }
        }
        std::ptr::null_mut()
    }

    pub fn open_gate(&self, ip: &str) -> bool {
        let sdk = match DahuaSdk::load() { Ok(s) => s, Err(_) => return false };
        let handle = match self.handle_for_ip(ip) {
            Some(h) => h,
            None => { warn!("Dahua open_gate: IP {ip} handle олдсонгүй"); return false; }
        };

        let mut strobe = NET_CTRL_OPEN_STROBE::default();
        strobe.nChannelId = 0;

        let ret = unsafe {
            (sdk.control_device)(
                handle,
                EM_CTRL_OPEN_STROBE,
                &mut strobe as *mut NET_CTRL_OPEN_STROBE as *mut c_void,
                5000,
            )
        };
        let err = unsafe { (sdk.get_last_error)() };

        if ret == 0 {
            error!("Dahua GATE FAIL | ip={ip} err={err:#x} — reconnecting");
            self.reconnect_single(ip);

            // Retry once after reconnect
            if let Some(new_handle) = self.handle_for_ip(ip) {
                let mut strobe2 = NET_CTRL_OPEN_STROBE::default();
                strobe2.nChannelId = 0;
                let ret2 = unsafe {
                    (sdk.control_device)(
                        new_handle,
                        EM_CTRL_OPEN_STROBE,
                        &mut strobe2 as *mut NET_CTRL_OPEN_STROBE as *mut c_void,
                        5000,
                    )
                };
                if ret2 == 0 {
                    error!("Dahua GATE RETRY FAIL | ip={ip}");
                }
                return ret2 != 0;
            }
            error!("Dahua GATE RETRY FAIL | ip={ip} — no handle after reconnect");
            return false;
        }

        ret != 0
    }

    fn reconnect_single(&self, ip: &str) {
        let _guard = self.reconnect_lock.lock().unwrap();
        let sdk = match DahuaSdk::load() { Ok(s) => s, Err(_) => return };

        let password = {
            self.cameras.lock().unwrap()
                .iter()
                .find(|c| c.ip == ip)
                .map(|c| c.password.clone())
                .unwrap_or_default()
        };

        println!("Dahua дахин холбогдож байна: {ip}");

        if let Some(old_handle) = self.handle_for_ip(ip) {
            unsafe { let _ = (sdk.logout)(old_handle); }
        }

        let handle = Self::connect_with_retry(sdk, ip, &password, &self.sdk_cfg);
        if !handle.is_null() {
            self.handle_map.lock().unwrap().insert(ip.to_string(), handle);
            let mut cams = self.cameras.lock().unwrap();
            if let Some(cam) = cams.iter_mut().find(|c| c.ip == ip) {
                cam.handle = handle;
            }
            println!("Dahua дахин амжилттай холбогдлоо: {ip}");
        } else {
            error!("Dahua дахин холбогдоход амжилтгүй: {ip}");
        }
    }

    pub fn check_connections(&self, cameras: &[(String, String)]) {
        for (ip, _) in cameras {
            let handle = self.handle_for_ip(ip);
            if handle.map(|h| h.is_null()).unwrap_or(true) {
                error!("Dahua HEARTBEAT | no handle for {ip} — reconnecting");
                self.reconnect_single(ip);
                continue;
            }

            let sdk_port = self.sdk_cfg.dahua_sdk_port;
            let addr = format!("{ip}:{sdk_port}");
            let reachable = match addr.parse::<std::net::SocketAddr>() {
                Ok(sock_addr) => std::net::TcpStream::connect_timeout(
                    &sock_addr,
                    std::time::Duration::from_secs(3),
                ).is_ok(),
                Err(_) => false,
            };

            if !reachable {
                error!("Dahua HEARTBEAT | {ip} unreachable — reconnecting");
                self.reconnect_single(ip);
            }
        }
    }
}
