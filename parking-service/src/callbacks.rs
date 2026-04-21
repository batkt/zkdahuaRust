//! Static `extern "system"` callbacks for AlprSDK.
//! These are the Rust equivalent of C# staticCallback1..8.
//! Being raw `extern "system" fn` pointers they can NEVER be GC'd.

use std::ffi::c_void;
use log::warn;

use crate::sdk::{RecogAllInfo, RecogAllInfoCallback, decode_plate, plate_color_name};
use crate::camera_manager::{CAMERA_MANAGER, PlateEvent};

// ─── Core handler ─────────────────────────────────────────────────────────────
// Mirrors the body shared across all C# RecogResultCallback1..8
// C# always does: loop, take first plate, fire-and-forget async, then return

fn handle_recog_event(handle: i32, p_recog: *const RecogAllInfo) {
    if p_recog.is_null() { return; }
    let recog = unsafe { &*p_recog };

    let n_plates = recog.plate_info.n_plate_num;
    if n_plates <= 0 { return; }

    // C# code: `for (int i = 0; i < nPlateNum; i++) { ... return; }` → always first plate only
    let plate_raw  = &recog.plate_info.p_plate[0];
    let plate_str  = decode_plate(&plate_raw.sz_license);
    let color_name = plate_color_name(plate_raw.plate_color);

    if plate_str.is_empty() { return; }

    println!("RecogResultCallback{} | mashinii Dugaar burtgegdlee: {} | color={}",
             handle + 1, plate_str, color_name);

    let mgr = match CAMERA_MANAGER.get() { Some(m) => m, None => return };

    let camera_ip = match mgr.ip_for_handle(handle) {
        Some(ip) => ip,
        None => {
            warn!("callback: handle {handle} has no IP (reconnecting?) — plate dropped");
            return;
        }
    };

    let event = PlateEvent { plate: plate_str, camera_ip, handle };
    if let Err(e) = mgr.plate_tx.try_send(event) {
        warn!("Plate event channel full for handle {handle}: {e}");
    }
}

// ─── Static callbacks 0..7 (= C# staticCallback1..8) ─────────────────────────
// Must be free `extern "system"` functions — NOT closures, NOT methods.

macro_rules! make_cb {
    ($name:ident, $idx:expr) => {
        pub unsafe extern "system" fn $name(
            p_recog: *const RecogAllInfo,
            _p_user: *mut c_void,
        ) {
            handle_recog_event($idx, p_recog);
        }
    };
}

make_cb!(callback_0, 0);
make_cb!(callback_1, 1);
make_cb!(callback_2, 2);
make_cb!(callback_3, 3);
make_cb!(callback_4, 4);
make_cb!(callback_5, 5);
make_cb!(callback_6, 6);
make_cb!(callback_7, 7);

/// Return the static callback function for a given handle index.
/// Mirrors the C# if/else chain: staticCallback1..8 per handle index.
pub fn callback_for_handle(handle: usize) -> Option<RecogAllInfoCallback> {
    match handle {
        0 => Some(callback_0),
        1 => Some(callback_1),
        2 => Some(callback_2),
        3 => Some(callback_3),
        4 => Some(callback_4),
        5 => Some(callback_5),
        6 => Some(callback_6),
        7 => Some(callback_7),
        _ => None,
    }
}
