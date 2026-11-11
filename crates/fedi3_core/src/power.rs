/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

//! Process-wide power profile for the local core.
//!
//! Mobile clients tell the core whether their UI is in the foreground,
//! backgrounded or in a doze-like state. Background workers stretch their
//! polling intervals accordingly: on a phone every avoided wakeup is radio and
//! battery time, while a foreground client keeps the latency-sensitive
//! cadence it always had.
//!
//! The profile is process-wide on purpose: a mobile process runs exactly one
//! core, and both the UI isolate and the Android keep-alive isolate address
//! the same instance.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;
use std::time::Duration;

pub const FOREGROUND: u8 = 0;
pub const BACKGROUND: u8 = 1;
pub const DOZE: u8 = 2;

static MODE: AtomicU8 = AtomicU8::new(FOREGROUND);
static CHANGED: OnceLock<tokio::sync::Notify> = OnceLock::new();
/// Whether a relay tunnel can carry traffic while the UI is away. A P2P-only
/// account has no relay fallback and must keep its swarm running.
static RELAY_AVAILABLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn changed_notify() -> &'static tokio::sync::Notify {
    CHANGED.get_or_init(tokio::sync::Notify::new)
}

pub fn mode() -> u8 {
    MODE.load(Ordering::Relaxed)
}

pub fn is_foreground() -> bool {
    mode() == FOREGROUND
}

/// Records whether a relay tunnel is configured (and therefore whether the
/// P2P swarm may be suspended while backgrounded).
pub fn set_relay_available(available: bool) {
    RELAY_AVAILABLE.store(available, Ordering::Relaxed);
}

pub fn relay_available() -> bool {
    RELAY_AVAILABLE.load(Ordering::Relaxed)
}

/// Sets the profile. Returns `false` for an unknown mode.
pub fn set_mode(mode: u8) -> bool {
    if mode > DOZE {
        return false;
    }
    MODE.store(mode, Ordering::Relaxed);
    // Wake every worker that is currently sleeping in a slow cadence so a
    // returning foreground client is served immediately.
    changed_notify().notify_waiters();
    true
}

/// Parses the public mode names used by the internal HTTP endpoint.
pub fn parse_mode(value: &str) -> Option<u8> {
    match value.trim().to_ascii_lowercase().as_str() {
        "foreground" | "fg" => Some(FOREGROUND),
        "background" | "bg" => Some(BACKGROUND),
        "doze" | "idle" => Some(DOZE),
        _ => None,
    }
}

/// Polling cadence for a worker under the current profile.
pub fn interval_secs(foreground: u64, background: u64, doze: u64) -> u64 {
    match mode() {
        FOREGROUND => foreground,
        BACKGROUND => background,
        _ => doze,
    }
    .max(1)
}

/// Completes when the profile changes, so sleeping workers can re-evaluate.
pub async fn changed() {
    changed_notify().notified().await;
}

/// Sleeps for the current cadence, returning early when the profile changes.
pub async fn sleep(foreground: u64, background: u64, doze: u64) {
    let secs = interval_secs(foreground, background, doze);
    tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(secs)) => {}
        _ = changed() => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cadence_follows_the_profile() {
        // Tests share the process-wide profile: restore it before asserting.
        set_mode(FOREGROUND);
        assert_eq!(interval_secs(3, 30, 120), 3);
        assert!(set_mode(BACKGROUND));
        assert_eq!(interval_secs(3, 30, 120), 30);
        assert!(set_mode(DOZE));
        assert_eq!(interval_secs(3, 30, 120), 120);
        assert!(set_mode(FOREGROUND));
        assert_eq!(interval_secs(0, 30, 120), 1);
        assert!(!set_mode(9));
        assert_eq!(parse_mode("background"), Some(BACKGROUND));
        assert_eq!(parse_mode(" nope "), None);
    }
}
