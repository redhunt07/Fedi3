/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayHttpRequest {
    pub id: String,
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body_b64: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RelayHttpResponse {
    pub id: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body_b64: String,
}

