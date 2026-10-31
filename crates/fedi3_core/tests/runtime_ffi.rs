/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#![allow(clippy::field_reassign_with_default, clippy::let_unit_value)]

mod common;

use common::*;
use fedi3_core::runtime::CoreStartConfig;
use std::ffi::{c_char, CStr, CString};

fn base_config(port: u16, dir: &std::path::Path) -> CoreStartConfig {
    let mut cfg = CoreStartConfig::default();
    cfg.username = "alice".to_string();
    cfg.domain = "example.test".to_string();
    cfg.public_base_url = format!("http://127.0.0.1:{port}");
    cfg.relay_ws = String::new();
    cfg.relay_token = None;
    cfg.bind = format!("127.0.0.1:{port}");
    cfg.data_dir = Some(dir.to_string_lossy().to_string());
    cfg.internal_token = Some(INTERNAL_TOKEN.to_string());
    cfg.max_body_bytes = Some(1024 * 1024);
    cfg.p2p = None;
    cfg.media = None;
    cfg.storage = None;
    cfg
}

fn call_start(cfg: &CoreStartConfig) -> (i32, u64, Option<String>) {
    let json = serde_json::to_string(cfg).expect("serialize config");
    let c_json = CString::new(json).expect("cstring");
    let mut handle: u64 = 0;
    let mut err: *mut c_char = std::ptr::null_mut();
    let code = fedi3_core::ffi::fedi3_core_start(c_json.as_ptr(), &mut handle, &mut err);
    let msg = if err.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string();
        fedi3_core::ffi::fedi3_core_free_cstring(err);
        Some(s)
    };
    (code, handle, msg)
}

fn call_stop(handle: u64) -> (i32, Option<String>) {
    let mut err: *mut c_char = std::ptr::null_mut();
    let code = fedi3_core::ffi::fedi3_core_stop(handle, &mut err);
    let msg = if err.is_null() {
        None
    } else {
        let s = unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string();
        fedi3_core::ffi::fedi3_core_free_cstring(err);
        Some(s)
    };
    (code, msg)
}

#[test]
fn r01_version_is_non_empty_and_freeable() {
    let p = fedi3_core::fedi3_core_version();
    assert!(!p.is_null());
    let v = unsafe { CStr::from_ptr(p) }.to_string_lossy().to_string();
    assert!(!v.is_empty());
    fedi3_core::fedi3_core_string_free(p);
}

#[test]
fn r02_invalid_json_returns_code_2() {
    let c = CString::new("{not json").unwrap();
    let mut handle: u64 = 0;
    let mut err: *mut c_char = std::ptr::null_mut();
    let code = fedi3_core::ffi::fedi3_core_start(c.as_ptr(), &mut handle, &mut err);
    assert_eq!(code, 2);
    assert!(!err.is_null());
    let msg = unsafe { CStr::from_ptr(err) }.to_string_lossy().to_string();
    assert!(msg.contains("invalid config json"));
    fedi3_core::ffi::fedi3_core_free_cstring(err);
    assert_eq!(handle, 0);
}

#[test]
fn r02b_null_arguments_return_code_1() {
    let mut handle: u64 = 0;
    let mut err: *mut c_char = std::ptr::null_mut();
    let code = fedi3_core::ffi::fedi3_core_start(std::ptr::null(), &mut handle, &mut err);
    assert_eq!(code, 1);
    fedi3_core::ffi::fedi3_core_free_cstring(err);
}

#[tokio::test]
async fn r03_valid_config_returns_only_after_bind() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-start");
    let port = free_port();
    let cfg = base_config(port, dir.path());
    let (code, handle, err) = call_start(&cfg);
    assert_eq!(code, 0, "start failed: {err:?}");
    assert!(handle > 0);
    let resp = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/healthz"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(call_stop(handle).0, 0);
}

#[tokio::test]
async fn r04_busy_port_returns_error_not_a_live_handle() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-busy");
    let port = free_port();
    let busy = std::net::TcpListener::bind(("127.0.0.1", port)).unwrap();
    let cfg = base_config(port, dir.path());
    let (code, handle, err) = call_start(&cfg);
    assert_ne!(code, 0);
    assert_eq!(handle, 0);
    let msg = err.unwrap_or_default();
    assert!(
        msg.to_lowercase().contains("bind") || msg.to_lowercase().contains("address"),
        "unexpected error: {msg}"
    );
    drop(busy);
}

#[test]
fn r05_stop_unknown_handle_is_an_error() {
    let (code, msg) = call_stop(999_999);
    assert_ne!(code, 0);
    assert!(msg.unwrap_or_default().contains("invalid handle"));
}

#[tokio::test]
async fn r06_two_cores_get_independent_handles_and_ports() {
    let dir_a = fedi3_core::test_support::TempDir::new("ffi-a");
    let dir_b = fedi3_core::test_support::TempDir::new("ffi-b");
    let port_a = free_port();
    let port_b = free_port();
    let (code_a, handle_a, _) = call_start(&base_config(port_a, dir_a.path()));
    let (code_b, handle_b, _) = call_start(&base_config(port_b, dir_b.path()));
    assert_eq!(code_a, 0);
    assert_eq!(code_b, 0);
    assert_ne!(handle_a, handle_b);
    assert_eq!(call_stop(handle_a).0, 0);
    assert_eq!(call_stop(handle_b).0, 0);
}

#[tokio::test]
async fn r08_stopping_twice_is_handled() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-stop2");
    let port = free_port();
    let (code, handle, _) = call_start(&base_config(port, dir.path()));
    assert_eq!(code, 0);
    assert_eq!(call_stop(handle).0, 0);
    let (code2, _) = call_stop(handle);
    assert_ne!(code2, 0);
}

#[tokio::test]
async fn r09_start_without_relay_token_still_serves_locally() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-norelay");
    let port = free_port();
    let mut cfg = base_config(port, dir.path());
    cfg.relay_ws = String::new();
    cfg.relay_token = None;
    let (code, handle, err) = call_start(&cfg);
    assert_eq!(code, 0, "relay must be optional: {err:?}");
    let resp = reqwest::Client::new()
        .get(format!("http://127.0.0.1:{port}/_fedi3/health"))
        .header("X-Fedi3-Internal", INTERNAL_TOKEN)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    assert_eq!(call_stop(handle).0, 0);
}

#[test]
fn r10_invalid_username_and_domain_are_rejected() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-invalid");
    let port = free_port();
    let mut cfg = base_config(port, dir.path());
    cfg.username = "../other".to_string();
    let (code, handle, err) = call_start(&cfg);
    assert_ne!(code, 0);
    assert_eq!(handle, 0);
    assert!(err.unwrap_or_default().contains("username"));

    let port2 = free_port();
    let mut cfg2 = base_config(port2, dir.path());
    cfg2.domain = "bad domain".to_string();
    let (code2, handle2, err2) = call_start(&cfg2);
    assert_ne!(code2, 0);
    assert_eq!(handle2, 0);
    assert!(err2.unwrap_or_default().contains("domain"));
}

#[test]
fn r10b_empty_username_is_rejected() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-empty-user");
    let port = free_port();
    let mut cfg = base_config(port, dir.path());
    cfg.username = String::new();
    let (code, _, _) = call_start(&cfg);
    assert_ne!(code, 0);
}

#[test]
fn r10c_username_too_long_is_rejected() {
    let dir = fedi3_core::test_support::TempDir::new("ffi-long-user");
    let port = free_port();
    let mut cfg = base_config(port, dir.path());
    cfg.username = "a".repeat(65);
    let (code, _, _) = call_start(&cfg);
    assert_ne!(code, 0);
}
