/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use serde::Serialize;

fn now_ms_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize)]
pub struct UiEvent {
    pub kind: String,
    pub ts_ms: u64,
    pub activity_type: Option<String>,
    pub activity_id: Option<String>,
}

impl UiEvent {
    pub fn new(kind: &str, activity_type: Option<String>, activity_id: Option<String>) -> Self {
        Self {
            kind: kind.to_string(),
            ts_ms: now_ms_u64(),
            activity_type,
            activity_id,
        }
    }
}
