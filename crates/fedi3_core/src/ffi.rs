/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use crate::runtime::{self, CoreStartConfig};
use std::ffi::{c_char, c_int, CStr, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

fn set_err(out_err: *mut *mut c_char, msg: String) {
    if out_err.is_null() {
        return;
    }
    let c = CString::new(msg).unwrap_or_else(|_| CString::new("ffi error").unwrap());
    unsafe {
        *out_err = c.into_raw();
    }
}

/// Run an FFI entry point with `catch_unwind`. A panic must never cross the `extern "C"`
/// boundary (since Rust 1.81 that aborts the whole process).
fn run_guarded<F>(out_err: *mut *mut c_char, name: &str, f: F) -> c_int
where
    F: FnOnce() -> c_int,
{
    // L-06: always clear the output pointer before writing, so a caller reusing the buffer
    // never sees a stale pointer.
    if !out_err.is_null() {
        unsafe {
            *out_err = std::ptr::null_mut();
        }
    }
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(code) => code,
        Err(_) => {
            set_err(out_err, format!("panic in {name}"));
            4
        }
    }
}

#[no_mangle]
pub extern "C" fn fedi3_core_start(
    config_json: *const c_char,
    out_handle: *mut u64,
    out_err: *mut *mut c_char,
) -> c_int {
    run_guarded(out_err, "fedi3_core_start", || {
        if config_json.is_null() || out_handle.is_null() {
            set_err(out_err, "null argument".to_string());
            return 1;
        }
        let cfg_str = unsafe { CStr::from_ptr(config_json) }
            .to_string_lossy()
            .to_string();
        let cfg: CoreStartConfig = match serde_json::from_str(&cfg_str) {
            Ok(v) => v,
            Err(e) => {
                set_err(out_err, format!("invalid config json: {e}"));
                return 2;
            }
        };
        match runtime::start(cfg) {
            Ok(handle) => {
                unsafe {
                    *out_handle = handle;
                }
                0
            }
            Err(e) => {
                set_err(out_err, format!("{e:#}"));
                3
            }
        }
    })
}

#[no_mangle]
pub extern "C" fn fedi3_core_stop(handle: u64, out_err: *mut *mut c_char) -> c_int {
    run_guarded(out_err, "fedi3_core_stop", || match runtime::stop(handle) {
        Ok(()) => 0,
        Err(e) => {
            set_err(out_err, format!("{e:#}"));
            1
        }
    })
}

#[no_mangle]
pub extern "C" fn fedi3_core_free_cstring(ptr: *mut c_char) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if ptr.is_null() {
            return;
        }
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panic_is_mapped_to_error_code() {
        let mut err: *mut c_char = std::ptr::null_mut();
        let code = run_guarded(&mut err, "test", || panic!("boom"));
        assert_eq!(code, 4);
        assert!(!err.is_null());
        let msg = unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string();
        assert!(msg.contains("panic"));
        unsafe {
            drop(CString::from_raw(err));
        }
    }

    #[test]
    fn output_error_pointer_is_cleared() {
        let mut err: *mut c_char = 1 as *mut c_char;
        let code = run_guarded(&mut err, "test", || 0);
        assert_eq!(code, 0);
        assert!(err.is_null());
    }
}
