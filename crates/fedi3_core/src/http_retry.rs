/*
 * SPDX-FileCopyrightText: 2026 RedHunt07 - FEDI3 Project
 * SPDX-License-Identifier: AGPL-3.0-only
 */

use anyhow::Result;
use rand::{thread_rng, Rng};
use reqwest::{RequestBuilder, Response, StatusCode};
use std::time::Duration;

use crate::net_metrics::NetMetrics;

pub async fn send_with_retry<F>(mut build: F, attempts: u32) -> Result<Response>
where
    F: FnMut() -> RequestBuilder,
{
    let max_attempts = attempts.clamp(1, 5);
    let mut backoff = Duration::from_millis(200);
    for attempt in 0..max_attempts {
        match build().send().await {
            Ok(resp) => {
                let status = resp.status();
                if should_retry_status(status) && attempt + 1 < max_attempts {
                    sleep_with_jitter(backoff).await;
                    backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
                    continue;
                }
                return Ok(resp);
            }
            Err(e) => {
                if attempt + 1 >= max_attempts {
                    return Err(e.into());
                }
                sleep_with_jitter(backoff).await;
                backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
            }
        }
    }
    unreachable!("retry loop should return or error");
}

pub async fn send_with_retry_metrics<F>(
    mut build: F,
    attempts: u32,
    metrics: &NetMetrics,
) -> Result<Response>
where
    F: FnMut() -> RequestBuilder,
{
    let max_attempts = attempts.clamp(1, 5);
    let mut backoff = Duration::from_millis(200);
    for attempt in 0..max_attempts {
        match build().send().await {
            Ok(resp) => {
                let status = resp.status();
                if should_retry_status(status) {
                    metrics.http_error();
                    if attempt + 1 < max_attempts {
                        sleep_with_jitter(backoff).await;
                        backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
                        continue;
                    }
                }
                return Ok(resp);
            }
            Err(e) => {
                if e.is_timeout() {
                    metrics.http_timeout();
                } else {
                    metrics.http_error();
                }
                if attempt + 1 >= max_attempts {
                    return Err(e.into());
                }
                sleep_with_jitter(backoff).await;
                backoff = backoff.saturating_mul(2).min(Duration::from_secs(5));
            }
        }
    }
    unreachable!("retry loop should return or error");
}

fn should_retry_status(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

async fn sleep_with_jitter(base: Duration) {
    let jitter_ms: u64 = thread_rng().gen_range(0..=200);
    let jitter = Duration::from_millis(jitter_ms);
    tokio::time::sleep(base + jitter).await;
}
