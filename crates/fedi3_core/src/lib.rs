/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use std::ffi::{c_char, CString};

pub mod ap;
pub mod delivery;
pub mod delivery_queue;
pub mod http_retry;
pub mod http_sig;
pub mod keys;
pub mod media_backend;
pub mod object_fetch;
pub mod storage_gc;
pub mod runtime;
pub mod social_db;
pub mod tunnel;
pub mod relay_bridge;
pub mod p2p;
pub mod p2p_sync;
pub mod relay_sync;
pub mod device_sync;
pub mod crypto_envelope;
pub mod webrtc_p2p;
pub mod legacy_sync;
pub mod net_metrics;
pub mod ui_events;
pub mod chat;
mod ffi;

#[no_mangle]
pub extern "C" fn fedi3_core_version() -> *mut c_char {
    CString::new(env!("CARGO_PKG_VERSION"))
        .expect("version is valid CString")
        .into_raw()
}

#[no_mangle]
pub extern "C" fn fedi3_core_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}
