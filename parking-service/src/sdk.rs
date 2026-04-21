//! FFI bindings to AlprSDK.dll
//! Struct layouts derived EXACTLY from the C# AlprSDK.cs header.
//!
//! Calling convention: CallingConvention.StdCall in C# →
//!   extern "system" in Rust (= stdcall on x86, default CC on x64)
//!
//! CharSet: StructLayout has no CharSet → defaults to Ansi
//! → all ByValTStr fields are 1 byte per char (ASCII/ANSI).

#![allow(non_snake_case, dead_code)]

use std::ffi::{c_int, c_uint, c_void};
use libloading::{Library, Symbol};
use once_cell::sync::OnceCell;

pub use std::os::raw::c_ushort;

// ─── XRect ────────────────────────────────────────────────────────────────────
// [StructLayout(LayoutKind.Sequential)]
// int left, right, top, bottom
// sizeof = 16
#[repr(C)]
#[derive(Clone, Default, Debug)]
pub struct XRect {
    pub left:   c_int,
    pub right:  c_int,
    pub top:    c_int,
    pub bottom: c_int,
}

// ─── PLATERESULT ──────────────────────────────────────────────────────────────
// offset  0: [ByValArray SizeConst=24] byte[] szLicense      → [u8; 24]
// offset 24: int   nLetterCount                              → i32
// offset 28: float fConfidence                               → f32
// offset 32: XRect rect                                      → 16 bytes
// offset 48: byte  plateColor                                → u8
// offset 49: byte  bDoublePlates                             → u8
// offset 50: byte  nDirection                                → u8
// offset 51: [ByValTStr SizeConst=33] string reserve (ANSI) → [u8; 33]
// sizeof = 84
#[repr(C)]
#[derive(Clone, Debug)]
pub struct PlateResult {
    pub sz_license:      [u8; 24],
    pub n_letter_count:  i32,
    pub f_confidence:    f32,
    pub rect:            XRect,
    pub plate_color:     u8,
    pub b_double_plates: u8,
    pub n_direction:     u8,
    pub reserve:         [u8; 33],
}

impl Default for PlateResult {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

// ─── LICENSE_PLATE ────────────────────────────────────────────────────────────
// offset  0: [ByValTStr SizeConst=20] string szTime (ANSI)  → [u8; 20]
// offset 20: int nProcessTime                               → i32
// offset 24: int nPlateNum                                  → i32
// offset 28: [ByValArray SizeConst=4] PLATERESULT[] pPlate  → [PlateResult; 4]
// sizeof = 20 + 4 + 4 + (84*4) = 364
#[repr(C)]
#[derive(Debug)]
pub struct LicensePlate {
    pub sz_time:        [u8; 20],
    pub n_process_time: i32,
    pub n_plate_num:    i32,
    pub p_plate:        [PlateResult; 4],
}

impl Default for LicensePlate {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

// ─── JPG_BYTES ────────────────────────────────────────────────────────────────
// offset  0: [ByValTStr SizeConst=20] string szTime (ANSI) → [u8; 20]
// offset 20: int    nBytesLen                              → i32
// offset 24: IntPtr pJpgBytes                              → *mut c_void (8 bytes x64)
//            (24 % 8 == 0 → correctly aligned)
// sizeof = 32
#[repr(C)]
#[derive(Debug)]
pub struct JpgBytes {
    pub sz_time:     [u8; 20],
    pub n_bytes_len: i32,
    pub p_jpg_bytes: *mut c_void,
}

impl Default for JpgBytes {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

// ─── RECOG_ALL_INFO ───────────────────────────────────────────────────────────
// offset   0: LICENSE_PLATE PlateInfo               → LicensePlate (364 bytes)
// offset 364: (4 bytes implicit padding — JpgBytes needs 8-byte alignment for IntPtr)
// offset 368: JPG_BYTES JpgBytes                    → JpgBytes (32 bytes)
// offset 400: [ByValTStr SizeConst=32] string nReserve (ANSI) → [u8; 32]
// sizeof = 432
//
// *** This is the struct the SDK passes to RecogAllInfoCallback ***
#[repr(C)]
#[derive(Debug)]
pub struct RecogAllInfo {
    pub plate_info: LicensePlate,
    pub _pad:       [u8; 4],    // ← alignment padding — REQUIRED, DO NOT REMOVE
    pub jpg_bytes:  JpgBytes,
    pub n_reserve:  [u8; 32],
}

impl Default for RecogAllInfo {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

// ─── DEVINFO ──────────────────────────────────────────────────────────────────
// No [StructLayout] attr → Sequential/Ansi (1 byte per char in ByValTStr)
//
// offset   0: [ByValTStr SizeConst=32]  szIP               → [u8; 32]
// offset  32: [ByValTStr SizeConst=128] szDevName          → [u8; 128]
// offset 160: [ByValTStr SizeConst=32]  szDevUid           → [u8; 32]
// offset 192: ushort uUseP2PConn                           → u16
// offset 194: ushort u16Port                               → u16
// offset 196: [ByValTStr SizeConst=64]  szUser             → [u8; 64]
// offset 260: [ByValTStr SizeConst=64]  szPwd              → [u8; 64]
// offset 324: [ByValTStr SizeConst=256] szPicturesSavePath → [u8; 256]
// offset 580: UInt16 u16AlprPort                           → u16
// offset 582: ushort lprDevType                            → u16
// offset 584: IntPtr hPullHandle                           → *mut c_void (584 % 8 == 0 ✓)
// sizeof = 592
#[repr(C)]
pub struct DevInfo {
    pub sz_ip:            [u8; 32],
    pub sz_dev_name:      [u8; 128],
    pub sz_dev_uid:       [u8; 32],
    pub u_use_p2p_conn:   u16,
    pub u16_port:         u16,
    pub sz_user:          [u8; 64],
    pub sz_pwd:           [u8; 64],
    pub sz_pictures_path: [u8; 256],
    pub u16_alpr_port:    u16,
    pub lpr_dev_type:     u16,
    pub h_pull_handle:    *mut c_void,
}

unsafe impl Send for DevInfo {}

impl Default for DevInfo {
    fn default() -> Self { unsafe { std::mem::zeroed() } }
}

impl DevInfo {
    pub fn new(ip: &str, port: u16, user: &str, pwd: &str, dev_type: u16) -> Self {
        let mut d = DevInfo::default();
        fill_ansi(&mut d.sz_ip,       ip);
        fill_ansi(&mut d.sz_dev_name, "IPC");
        fill_ansi(&mut d.sz_user,     user);
        fill_ansi(&mut d.sz_pwd,      pwd);
        d.u16_port      = port;
        d.u16_alpr_port = port;   // GZ type uses this as the HTTPS API port
        d.lpr_dev_type  = dev_type;
        d
    }
}

// ─── EAPIClientType enum values ───────────────────────────────────────────────
pub const E_CLIENT_NORMAL: c_int = 0;
pub const E_CLIENT_DEV_OCX: c_int = 1;
pub const E_CLIENT_DEMO: c_int = 2;

// ─── ELPRDevType ─────────────────────────────────────────────────────────────
pub const LPR_DEV_UNKNOWN:     u16 = 0;
pub const LPR_DEV_JL:          u16 = 1;
pub const LPR_DEV_GZ:          u16 = 2;  // ← used in C# code
pub const LPR_DEV_GZ_CAR_SPACE: u16 = 3;

// ─── Callback types ───────────────────────────────────────────────────────────
//
// CallingConvention.StdCall + CharSet.Unicode

/// void RecogAllInfoCallback(ref RECOG_ALL_INFO, IntPtr pUserData)
pub type RecogAllInfoCallback =
    unsafe extern "system" fn(p_recog: *const RecogAllInfo, p_user_data: *mut c_void);

/// void ServerFindCallback(int, string, string, IntPtr, ushort, ushort, ...)
/// CharSet.Unicode → string params are *const u16 (UTF-16LE)
pub type ServerFindCallback =
    unsafe extern "system" fn(
        n_device_type: c_int,
        p_device_name: *const u16,
        p_ip:          *const u16,
        mac_addr:      *mut c_void,
        w_port_web:    c_ushort,
        w_port_listen: c_ushort,
        p_sub_mask:    *const u16,
        p_gateway:     *const u16,
        p_multi_addr:  *const u16,
        p_dns_addr:    *const u16,
        w_multi_port:  c_ushort,
        n_channel_num: c_int,
        n_find_count:  c_int,
        dw_device_id:  c_int,
    );

// ─── AlprSdk — loaded DLL ─────────────────────────────────────────────────────

static SDK: OnceCell<AlprSdk> = OnceCell::new();

pub struct AlprSdk {
    _lib: Library,

    pub startup:             unsafe extern "system" fn(*mut c_void, c_uint) -> c_int,
    pub search_all_cameras:  unsafe extern "system" fn(c_uint, ServerFindCallback) -> c_int,
    pub init_handle:         unsafe extern "system" fn(c_int, *mut c_void) -> c_int,
    pub uninit_handle:       unsafe extern "system" fn(c_int) -> c_int,
    pub connect_dev:         unsafe extern "system" fn(c_int, *const DevInfo, c_int) -> c_int,
    pub disconnect_dev:      unsafe extern "system" fn(c_int) -> c_int,
    pub start_video:         unsafe extern "system" fn(c_int) -> c_int,
    pub stop_video:          unsafe extern "system" fn(c_int) -> c_int,
    pub create_recog_task:   unsafe extern "system" fn(c_int, RecogAllInfoCallback, *mut c_void) -> c_int,
    pub clear_recog_task:    unsafe extern "system" fn(c_int) -> c_int,
    pub open_gate:           unsafe extern "system" fn(c_int) -> c_int,
    pub send_heartbeat:      unsafe extern "system" fn(c_int) -> c_int,
    pub set_connect_timeout: unsafe extern "system" fn(c_int, c_int) -> c_int,
    pub trans2screen:        unsafe extern "system" fn(
                                 c_int, c_int, c_int, *const u8,
                                 c_int, *const u8, c_int, *const u8,
                                 c_int, *const u8,
                             ) -> c_int,
    pub comm_transparent:    unsafe extern "system" fn(c_int, *const u8, c_int) -> c_int,
}

unsafe impl Send for AlprSdk {}
unsafe impl Sync for AlprSdk {}

impl AlprSdk {
    /// Load AlprSDK.dll from same directory as the exe.
    pub fn load() -> anyhow::Result<&'static Self> {
        SDK.get_or_try_init(|| unsafe {
            let exe_dir = std::env::current_exe()?
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .to_owned();
            let dll_path = exe_dir.join("AlprSDK.dll");

            // LOAD_WITH_ALTERED_SEARCH_PATH (0x8): when loading a DLL by absolute path,
            // Windows first searches the DLL's own directory for its dependencies.
            // This ensures libiconv, opencv, HHNetClient, etc. are found next to AlprSDK.dll.
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = dll_path.as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            // LOAD_WITH_ALTERED_SEARCH_PATH: Windows searches the DLL's own directory
            // for its dependencies before the application directory.
            // This finds libiconv, opencv, HHNetClient, etc. next to AlprSDK.dll.
            let handle = windows_sys::Win32::System::LibraryLoader::LoadLibraryExW(
                wide.as_ptr(),
                0,   // hFile must be 0 (HANDLE = isize in windows-sys)
                8,   // LOAD_WITH_ALTERED_SEARCH_PATH
            );
            if handle == 0 {
                let code = windows_sys::Win32::Foundation::GetLastError();
                anyhow::bail!("Cannot load {}: WinError={}", dll_path.display(), code);
            }
            // Wrap the raw HMODULE (isize) in a libloading Library
            let lib = libloading::os::windows::Library::from_raw(handle);
            let lib: Library = lib.into();

            macro_rules! sym {
                ($name:literal, $T:ty) => {{
                    let s: Symbol<$T> = lib.get($name)
                        .map_err(|e| anyhow::anyhow!("Symbol {} not found: {e}", stringify!($name)))?;
                    *s
                }};
            }

            Ok(AlprSdk {
                startup:             sym!(b"AlprSDK_Startup\0",                 unsafe extern "system" fn(*mut c_void, c_uint) -> c_int),
                search_all_cameras:  sym!(b"AlprSDK_SearchAllCameras\0",        unsafe extern "system" fn(c_uint, ServerFindCallback) -> c_int),
                init_handle:         sym!(b"AlprSDK_InitHandle\0",              unsafe extern "system" fn(c_int, *mut c_void) -> c_int),
                uninit_handle:       sym!(b"AlprSDK_UnInitHandle\0",            unsafe extern "system" fn(c_int) -> c_int),
                connect_dev:         sym!(b"AlprSDK_ConnectDev\0",              unsafe extern "system" fn(c_int, *const DevInfo, c_int) -> c_int),
                disconnect_dev:      sym!(b"AlprSDK_DisConnectDev\0",           unsafe extern "system" fn(c_int) -> c_int),
                start_video:         sym!(b"AlprSDK_StartVideo\0",              unsafe extern "system" fn(c_int) -> c_int),
                stop_video:          sym!(b"AlprSDK_StopVideo\0",               unsafe extern "system" fn(c_int) -> c_int),
                create_recog_task:   sym!(b"AlprSDK_CreateRecogAllInfoTask\0",  unsafe extern "system" fn(c_int, RecogAllInfoCallback, *mut c_void) -> c_int),
                clear_recog_task:    sym!(b"AlprSDK_ClearRecogAllInfoTask\0",   unsafe extern "system" fn(c_int) -> c_int),
                open_gate:           sym!(b"AlprSDK_OpenGate\0",                unsafe extern "system" fn(c_int) -> c_int),
                send_heartbeat:      sym!(b"AlprSDK_SendHeartBeat\0",           unsafe extern "system" fn(c_int) -> c_int),
                set_connect_timeout: sym!(b"AlprSDK_SetConnectTimeout\0",       unsafe extern "system" fn(c_int, c_int) -> c_int),
                trans2screen:        sym!(b"AlprSDK_Trans2Screen\0",            unsafe extern "system" fn(c_int,c_int,c_int,*const u8,c_int,*const u8,c_int,*const u8,c_int,*const u8) -> c_int),
                comm_transparent:    sym!(b"AlprSDK_CommTransparentTransfer\0", unsafe extern "system" fn(c_int, *const u8, c_int) -> c_int),
                _lib: lib,
            })
        })
    }
}

// ─── Layout verification tests ───────────────────────────────────────────────
// Run: cargo test -- --nocapture
// These will FAIL TO COMPILE if sizes are wrong — fix this BEFORE deploying.
#[cfg(test)]
mod layout_tests {
    use super::*;
    use std::mem::{size_of, offset_of};

    #[test]
    fn struct_sizes() {
        // Must match sizeof() in C# on x64 Windows
        assert_eq!(size_of::<XRect>(),       16,  "XRect");
        assert_eq!(size_of::<PlateResult>(), 84,  "PlateResult");
        assert_eq!(size_of::<LicensePlate>(),364, "LicensePlate");
        assert_eq!(size_of::<JpgBytes>(),    32,  "JpgBytes");
        assert_eq!(size_of::<RecogAllInfo>(),432, "RecogAllInfo");
        assert_eq!(size_of::<DevInfo>(),     592, "DevInfo");

        println!("All struct sizes match C# header ✓");
    }

    #[test]
    fn plate_result_offsets() {
        assert_eq!(offset_of!(PlateResult, sz_license),     0);
        assert_eq!(offset_of!(PlateResult, n_letter_count), 24);
        assert_eq!(offset_of!(PlateResult, f_confidence),   28);
        assert_eq!(offset_of!(PlateResult, rect),           32);
        assert_eq!(offset_of!(PlateResult, plate_color),    48);
        assert_eq!(offset_of!(PlateResult, b_double_plates),49);
        assert_eq!(offset_of!(PlateResult, n_direction),    50);
        assert_eq!(offset_of!(PlateResult, reserve),        51);
    }

    #[test]
    fn recog_all_info_offsets() {
        assert_eq!(offset_of!(RecogAllInfo, plate_info), 0);
        assert_eq!(offset_of!(RecogAllInfo, _pad),       364);
        assert_eq!(offset_of!(RecogAllInfo, jpg_bytes),  368);
        assert_eq!(offset_of!(RecogAllInfo, n_reserve),  400);
    }

    #[test]
    fn devinfo_offsets() {
        assert_eq!(offset_of!(DevInfo, sz_ip),            0);
        assert_eq!(offset_of!(DevInfo, sz_dev_name),      32);
        assert_eq!(offset_of!(DevInfo, sz_dev_uid),       160);
        assert_eq!(offset_of!(DevInfo, u_use_p2p_conn),   192);
        assert_eq!(offset_of!(DevInfo, u16_port),         194);
        assert_eq!(offset_of!(DevInfo, sz_user),          196);
        assert_eq!(offset_of!(DevInfo, sz_pwd),           260);
        assert_eq!(offset_of!(DevInfo, sz_pictures_path), 324);
        assert_eq!(offset_of!(DevInfo, u16_alpr_port),    580);
        assert_eq!(offset_of!(DevInfo, lpr_dev_type),     582);
        assert_eq!(offset_of!(DevInfo, h_pull_handle),    584);
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Copy ANSI string into a fixed-size byte array (null-terminated)
pub fn fill_ansi(dst: &mut [u8], src: &str) {
    dst.fill(0);
    let bytes = src.as_bytes();
    let len = bytes.len().min(dst.len().saturating_sub(1));
    dst[..len].copy_from_slice(&bytes[..len]);
}

/// Decode a null-terminated byte array from SDK → Rust String.
/// ZK cameras in Mongolia typically send plate numbers as UTF-8 or GB2312.
pub fn decode_plate(raw: &[u8]) -> String {
    let end = raw.iter().position(|&b| b == 0).unwrap_or(raw.len());
    std::str::from_utf8(&raw[..end])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| String::from_utf8_lossy(&raw[..end]).trim().to_string())
}

/// Read a null-terminated UTF-16LE string from a raw pointer
/// (used to decode pIP, pDeviceName from ServerFindCallback)
pub unsafe fn read_wide_str(ptr: *const u16) -> Option<String> {
    if ptr.is_null() { return None; }
    let mut len = 0usize;
    while *ptr.add(len) != 0 { len += 1; }
    let slice = std::slice::from_raw_parts(ptr, len);
    Some(String::from_utf16_lossy(slice))
}

/// Plate color code → human readable (matches C# colordic)
pub fn plate_color_name(code: u8) -> &'static str {
    match code {
        0   => "Black",
        20  => "Green",
        30  => "Blue",
        50  => "Yellow",
        255 => "White",
        _   => "Unknown",
    }
}
