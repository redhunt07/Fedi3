/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Default)]
pub struct NetMetrics {
    pub relay_connected: AtomicBool,
    pub relay_rx_bytes: AtomicU64,
    pub relay_tx_bytes: AtomicU64,
    pub relay_last_change_ms: AtomicU64,
    pub relay_rtt_ema_ms: AtomicU64,
    pub relay_handler_wait_ema_ms: AtomicU64,
    relay_last_error: Mutex<Option<String>>,

    pub p2p_enabled: AtomicBool,
    pub p2p_connected_peers: AtomicU64,
    pub p2p_active_peers: AtomicU64,
    pub p2p_rx_bytes: AtomicU64,
    pub p2p_tx_bytes: AtomicU64,
    pub p2p_rtt_ema_ms: AtomicU64,

    pub mailbox_active_peers: AtomicU64,
    pub mailbox_rx_bytes: AtomicU64,
    pub mailbox_tx_bytes: AtomicU64,
    pub mailbox_rtt_ema_ms: AtomicU64,

    pub webrtc_sessions: AtomicU64,
    pub webrtc_active_peers: AtomicU64,
    pub webrtc_rx_bytes: AtomicU64,
    pub webrtc_tx_bytes: AtomicU64,

    pub auth_failures: AtomicU64,
    pub rate_limit_hits: AtomicU64,
    pub http_timeouts: AtomicU64,
    pub http_errors: AtomicU64,
    pub chat_bundle_skipped_backoff: AtomicU64,
    pub timeline_filtered_non_note_total: AtomicU64,
    pub timeline_dedup_dropped_total: AtomicU64,
    pub timeline_local_items_total: AtomicU64,
    pub search_result_type_mismatch_total: AtomicU64,
    pub chat_group_membership_conflict_total: AtomicU64,
    pub transport_failover_total: AtomicU64,
    pub transport_failover_timeout_total: AtomicU64,
    pub transport_failover_rtt_total: AtomicU64,
    pub transport_failover_queue_total: AtomicU64,
    pub transport_failover_error_total: AtomicU64,
    pub transport_recover_total: AtomicU64,
    pub relay_preferred_active_peers: AtomicU64,
    pub relay_preferred_until_ms: AtomicU64,
    pub chat_path_latency_ema_ms: AtomicU64,
    pub timeline_fill_latency_ema_ms: AtomicU64,
    pub upnp_map_success_total: AtomicU64,
    pub upnp_map_fail_total: AtomicU64,
    pub upnp_map_renew_total: AtomicU64,

    p2p_seen: Mutex<HashMap<String, u64>>,
    mailbox_seen: Mutex<HashMap<String, u64>>,
    webrtc_seen: Mutex<HashMap<String, u64>>,
    last_failover_reason: Mutex<Option<String>>,
}

impl NetMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_relay_connected(&self, v: bool) {
        self.relay_connected.store(v, Ordering::Relaxed);
        self.relay_last_change_ms.store(now_ms(), Ordering::Relaxed);
        if v {
            let mut g = self.relay_last_error.lock().unwrap();
            *g = None;
        }
    }

    pub fn set_relay_error(&self, err: String) {
        self.set_relay_connected(false);
        let mut g = self.relay_last_error.lock().unwrap();
        *g = Some(err);
    }

    pub fn relay_rx_add(&self, n: u64) {
        self.relay_rx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn relay_tx_add(&self, n: u64) {
        self.relay_tx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn relay_rtt_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.relay_rtt_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.relay_rtt_ema_ms.store(next, Ordering::Relaxed);
    }

    pub fn relay_handler_wait_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.relay_handler_wait_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.relay_handler_wait_ema_ms
            .store(next, Ordering::Relaxed);
    }

    pub fn set_p2p_enabled(&self, v: bool) {
        self.p2p_enabled.store(v, Ordering::Relaxed);
    }

    pub fn p2p_connected_peers_set(&self, n: u64) {
        self.p2p_connected_peers.store(n, Ordering::Relaxed);
    }

    pub fn p2p_peer_seen(&self, peer_id: &str) {
        let mut g = self.p2p_seen.lock().unwrap();
        g.insert(peer_id.to_string(), now_ms());
        self.p2p_active_peers
            .store(g.len() as u64, Ordering::Relaxed);
    }

    pub fn p2p_rx_add(&self, n: u64) {
        self.p2p_rx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn p2p_tx_add(&self, n: u64) {
        self.p2p_tx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn p2p_rtt_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.p2p_rtt_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.p2p_rtt_ema_ms.store(next, Ordering::Relaxed);
    }

    pub fn mailbox_peer_seen(&self, peer_id: &str) {
        let mut g = self.mailbox_seen.lock().unwrap();
        g.insert(peer_id.to_string(), now_ms());
        self.mailbox_active_peers
            .store(g.len() as u64, Ordering::Relaxed);
    }

    pub fn mailbox_rx_add(&self, n: u64) {
        self.mailbox_rx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn mailbox_tx_add(&self, n: u64) {
        self.mailbox_tx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn mailbox_rtt_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.mailbox_rtt_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.mailbox_rtt_ema_ms.store(next, Ordering::Relaxed);
    }

    pub fn webrtc_sessions_set(&self, n: u64) {
        self.webrtc_sessions.store(n, Ordering::Relaxed);
    }

    pub fn webrtc_peer_seen(&self, peer_id: &str) {
        let mut g = self.webrtc_seen.lock().unwrap();
        g.insert(peer_id.to_string(), now_ms());
        self.webrtc_active_peers
            .store(g.len() as u64, Ordering::Relaxed);
    }

    pub fn webrtc_rx_add(&self, n: u64) {
        self.webrtc_rx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn webrtc_tx_add(&self, n: u64) {
        self.webrtc_tx_bytes.fetch_add(n, Ordering::Relaxed);
    }

    pub fn auth_failure(&self) {
        self.auth_failures.fetch_add(1, Ordering::Relaxed);
    }

    pub fn rate_limit_hit(&self) {
        self.rate_limit_hits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn http_timeout(&self) {
        self.http_timeouts.fetch_add(1, Ordering::Relaxed);
    }

    pub fn http_error(&self) {
        self.http_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn chat_bundle_backoff_skip(&self) {
        self.chat_bundle_skipped_backoff
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn timeline_filtered_non_note(&self) {
        self.timeline_filtered_non_note_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn timeline_dedup_dropped(&self) {
        self.timeline_dedup_dropped_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn timeline_local_item_seen(&self) {
        self.timeline_local_items_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn search_result_type_mismatch(&self) {
        self.search_result_type_mismatch_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn chat_group_membership_conflict(&self) {
        self.chat_group_membership_conflict_total
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn transport_failover(&self, reason: &str) {
        self.transport_failover_total.fetch_add(1, Ordering::Relaxed);
        let reason = reason.trim().to_ascii_lowercase();
        if reason.contains("timeout") {
            self.transport_failover_timeout_total
                .fetch_add(1, Ordering::Relaxed);
        } else if reason.contains("rtt") || reason.contains("latency") {
            self.transport_failover_rtt_total
                .fetch_add(1, Ordering::Relaxed);
        } else if reason.contains("queue") {
            self.transport_failover_queue_total
                .fetch_add(1, Ordering::Relaxed);
        } else {
            self.transport_failover_error_total
                .fetch_add(1, Ordering::Relaxed);
        }
        let mut g = self.last_failover_reason.lock().unwrap();
        *g = Some(reason);
    }

    pub fn transport_recover(&self) {
        self.transport_recover_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_relay_preferred_state(&self, active_peers: u64, until_ms: u64, reason: Option<&str>) {
        self.relay_preferred_active_peers
            .store(active_peers, Ordering::Relaxed);
        self.relay_preferred_until_ms
            .store(until_ms, Ordering::Relaxed);
        if let Some(r) = reason {
            let mut g = self.last_failover_reason.lock().unwrap();
            *g = Some(r.trim().to_string());
        }
    }

    pub fn chat_path_latency_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.chat_path_latency_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.chat_path_latency_ema_ms.store(next, Ordering::Relaxed);
    }

    pub fn timeline_fill_latency_update(&self, ms: u64) {
        if ms == 0 {
            return;
        }
        let prev = self.timeline_fill_latency_ema_ms.load(Ordering::Relaxed);
        let next = if prev == 0 {
            ms
        } else {
            (prev.saturating_mul(7).saturating_add(ms)) / 8
        };
        self.timeline_fill_latency_ema_ms
            .store(next, Ordering::Relaxed);
    }

    pub fn upnp_map_success(&self) {
        self.upnp_map_success_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn upnp_map_fail(&self) {
        self.upnp_map_fail_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn upnp_map_renew(&self) {
        self.upnp_map_renew_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn prune_seen(&self, window_ms: u64) {
        let cutoff = now_ms().saturating_sub(window_ms);
        {
            let mut g = self.p2p_seen.lock().unwrap();
            g.retain(|_, t| *t >= cutoff);
            self.p2p_active_peers
                .store(g.len() as u64, Ordering::Relaxed);
        }
        {
            let mut g = self.mailbox_seen.lock().unwrap();
            g.retain(|_, t| *t >= cutoff);
            self.mailbox_active_peers
                .store(g.len() as u64, Ordering::Relaxed);
        }
        {
            let mut g = self.webrtc_seen.lock().unwrap();
            g.retain(|_, t| *t >= cutoff);
            self.webrtc_active_peers
                .store(g.len() as u64, Ordering::Relaxed);
        }
    }

    pub fn snapshot_json(&self) -> serde_json::Value {
        let last_error = self.relay_last_error.lock().unwrap().clone();
        let last_failover_reason = self.last_failover_reason.lock().unwrap().clone();
        self.prune_seen(60_000);
        serde_json::json!({
            "ts_ms": now_ms(),
            "relay": {
                "connected": self.relay_connected.load(Ordering::Relaxed),
                "rx_bytes": self.relay_rx_bytes.load(Ordering::Relaxed),
                "tx_bytes": self.relay_tx_bytes.load(Ordering::Relaxed),
                "last_change_ms": self.relay_last_change_ms.load(Ordering::Relaxed),
                // Backward-compatible key. This is ping/pong tunnel RTT EMA.
                "rtt_ms": self.relay_rtt_ema_ms.load(Ordering::Relaxed),
                "ping_rtt_ms": self.relay_rtt_ema_ms.load(Ordering::Relaxed),
                "handler_wait_ms": self.relay_handler_wait_ema_ms.load(Ordering::Relaxed),
                "last_error": last_error,
            },
            "mailbox": {
                "active_peers": self.mailbox_active_peers.load(Ordering::Relaxed),
                "rx_bytes": self.mailbox_rx_bytes.load(Ordering::Relaxed),
                "tx_bytes": self.mailbox_tx_bytes.load(Ordering::Relaxed),
                "rtt_ms": self.mailbox_rtt_ema_ms.load(Ordering::Relaxed),
            },
            "errors": {
                "auth_failures": self.auth_failures.load(Ordering::Relaxed),
                "rate_limit_hits": self.rate_limit_hits.load(Ordering::Relaxed),
                "http_timeouts": self.http_timeouts.load(Ordering::Relaxed),
                "http_errors": self.http_errors.load(Ordering::Relaxed),
                "chat_bundle_skipped_backoff": self
                    .chat_bundle_skipped_backoff
                    .load(Ordering::Relaxed),
                "timeline_filtered_non_note_total": self
                    .timeline_filtered_non_note_total
                    .load(Ordering::Relaxed),
                "timeline_dedup_dropped_total": self
                    .timeline_dedup_dropped_total
                    .load(Ordering::Relaxed),
                "timeline_local_items_total": self
                    .timeline_local_items_total
                    .load(Ordering::Relaxed),
                "search_result_type_mismatch_total": self
                    .search_result_type_mismatch_total
                    .load(Ordering::Relaxed),
                "chat_group_membership_conflict_total": self
                    .chat_group_membership_conflict_total
                    .load(Ordering::Relaxed),
                "transport_failover_total": self
                    .transport_failover_total
                    .load(Ordering::Relaxed),
                "transport_recover_total": self
                    .transport_recover_total
                    .load(Ordering::Relaxed),
            },
            "failover": {
                "relay_preferred_active_peers": self
                    .relay_preferred_active_peers
                    .load(Ordering::Relaxed),
                "relay_preferred_until_ms": self
                    .relay_preferred_until_ms
                    .load(Ordering::Relaxed),
                "last_failover_reason": last_failover_reason,
            },
            "latency": {
                "chat_path_latency_ms": self
                    .chat_path_latency_ema_ms
                    .load(Ordering::Relaxed),
                "timeline_fill_latency_ms": self
                    .timeline_fill_latency_ema_ms
                    .load(Ordering::Relaxed),
            },
            "upnp": {
                "map_success_total": self.upnp_map_success_total.load(Ordering::Relaxed),
                "map_fail_total": self.upnp_map_fail_total.load(Ordering::Relaxed),
                "map_renew_total": self.upnp_map_renew_total.load(Ordering::Relaxed),
            }
        })
    }
}
