/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::runtime::{self, CoreStartConfig};
use std::ffi::{c_char, c_int, CStr, CString};

fn set_err(out_err: *mut *mut c_char, msg: String) {
    if out_err.is_null() {
        return;
    }
    let c = CString::new(msg).unwrap_or_else(|_| CString::new("ffi error").unwrap());
    unsafe {
        *out_err = c.into_raw();
    }
}

#[no_mangle]
pub extern "C" fn fedi3_core_start(config_json: *const c_char, out_handle: *mut u64, out_err: *mut *mut c_char) -> c_int {
    if config_json.is_null() || out_handle.is_null() {
        set_err(out_err, "null argument".to_string());
        return 1;
    }
    let cfg_str = unsafe { CStr::from_ptr(config_json) }.to_string_lossy().to_string();
    let cfg: CoreStartConfig = match serde_json::from_str(&cfg_str) {
        Ok(v) => v,
        Err(e) => {
            set_err(out_err, format!("invalid config json: {e}"));
            return 2;
        }
    };
    match runtime::start(cfg) {
        Ok(handle) => {
            unsafe { *out_handle = handle; }
            0
        }
        Err(e) => {
            set_err(out_err, format!("{e:#}"));
            3
        }
    }
}

#[no_mangle]
pub extern "C" fn fedi3_core_stop(handle: u64, out_err: *mut *mut c_char) -> c_int {
    match runtime::stop(handle) {
        Ok(()) => 0,
        Err(e) => {
            set_err(out_err, format!("{e:#}"));
            1
        }
    }
}

#[no_mangle]
pub extern "C" fn fedi3_core_free_cstring(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}
