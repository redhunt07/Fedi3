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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_activity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_object_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version_ts: Option<u64>,
}

impl UiEvent {
    pub fn new(kind: &str, activity_type: Option<String>, activity_id: Option<String>) -> Self {
        Self {
            kind: kind.to_string(),
            ts_ms: now_ms_u64(),
            activity_type,
            activity_id,
            object_id: None,
            target_activity_id: None,
            target_object_id: None,
            actor_id: None,
            kind_scope: None,
            op: None,
            version_ts: None,
        }
    }

    pub fn with_reducer_fields(
        mut self,
        object_id: Option<String>,
        target_activity_id: Option<String>,
        target_object_id: Option<String>,
        actor_id: Option<String>,
        kind_scope: Option<String>,
        op: Option<String>,
    ) -> Self {
        self.object_id = object_id;
        self.target_activity_id = target_activity_id;
        self.target_object_id = target_object_id;
        self.actor_id = actor_id;
        self.kind_scope = kind_scope;
        self.op = op;
        self.version_ts = Some(now_ms_u64());
        self
    }
}
