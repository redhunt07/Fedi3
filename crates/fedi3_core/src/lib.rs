/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// Clippy allows for pre-existing stylistic lints across the crate (kept to avoid a broad
// cosmetic refactor; correctness lints remain enabled).
#![allow(
    clippy::manual_clamp,
    clippy::result_large_err,
    clippy::single_match,
    clippy::not_unsafe_ptr_arg_deref,
    clippy::collapsible_if,
    clippy::collapsible_match,
    clippy::type_complexity,
    clippy::clone_on_copy,
    clippy::too_many_arguments,
    clippy::redundant_closure,
    clippy::needless_as_bytes,
    clippy::needless_question_mark,
    clippy::new_without_default,
    clippy::identity_op,
    clippy::let_and_return,
    clippy::let_unit_value,
    clippy::if_same_then_else,
    clippy::double_must_use,
    clippy::needless_update,
    clippy::useless_borrows_in_formatting,
    clippy::redundant_pattern_matching,
    clippy::should_implement_trait,
    clippy::manual_dangling_ptr,
    clippy::manual_ignore_case_cmp
)]

use std::ffi::{c_char, CString};

pub mod ap;
pub mod chat;
pub mod crypto_envelope;
pub mod delivery;
pub mod delivery_queue;
pub mod device_sync;
pub mod ffi;
pub mod http_retry;
pub mod http_sig;
pub mod keys;
pub mod legacy_sync;
pub mod media_backend;
pub mod net_metrics;
pub mod object_fetch;
pub mod p2p;
pub mod p2p_sync;
pub mod relay_bridge;
pub mod relay_sync;
pub mod runtime;
pub mod social_db;
pub mod storage_gc;
pub mod tunnel;
pub mod ui_events;
pub mod webrtc_p2p;

/// Minimal temp-dir helper for tests (no extra dependency needed).
#[doc(hidden)]
pub mod test_support {
    use std::path::{Path, PathBuf};

    pub struct TempDir(PathBuf);

    impl TempDir {
        pub fn new(prefix: &str) -> Self {
            let mut b = [0u8; 8];
            use rand::RngCore as _;
            rand::rngs::OsRng.fill_bytes(&mut b);
            let name = format!(
                "fedi3-test-{prefix}-{}-{}",
                std::process::id(),
                b.iter().map(|v| format!("{v:02x}")).collect::<String>()
            );
            let path = std::env::temp_dir().join(name);
            std::fs::create_dir_all(&path).expect("create temp dir");
            Self(path)
        }

        pub fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

#[no_mangle]
pub extern "C" fn fedi3_core_version() -> *mut c_char {
    std::panic::catch_unwind(|| {
        CString::new(env!("CARGO_PKG_VERSION"))
            .unwrap_or_else(|_| CString::new("unknown").unwrap())
            .into_raw()
    })
    .unwrap_or(std::ptr::null_mut())
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
