use anyhow::{Context, Result};
use libp2p::relay;
use std::{num::NonZeroU32, time::Duration};

/// A circuit carries complete Fedi3 frames, not just a hole-punch handshake.
/// libp2p's 128 KiB / 2 minute defaults interrupt ordinary chat and sync traffic.
#[derive(Debug, Clone)]
pub struct RelayLimits {
    max_circuits: usize,
    max_per_peer: usize,
    max_bytes: u64,
    max_secs: u64,
}

impl RelayLimits {
    pub fn from_env() -> Result<Self> {
        Self::parse(|key| std::env::var(key).ok())
    }

    fn parse(mut get: impl FnMut(&str) -> Option<String>) -> Result<Self> {
        let mut number = |key: &str, default: u64| -> Result<u64> {
            let value = match get(key) {
                Some(raw) => raw
                    .parse::<u64>()
                    .with_context(|| format!("invalid {key}"))?,
                None => default,
            };
            anyhow::ensure!(value > 0, "{key} must be positive");
            Ok(value)
        };
        let max_circuits = number("FEDI3_P2P_RELAY_MAX_CIRCUITS", 64)?;
        let max_per_peer = number("FEDI3_P2P_RELAY_MAX_CIRCUITS_PER_PEER", 8)?;
        let max_bytes = number("FEDI3_P2P_RELAY_MAX_CIRCUIT_BYTES", 16 * 1024 * 1024)?;
        let max_secs = number("FEDI3_P2P_RELAY_MAX_CIRCUIT_SECS", 900)?;
        anyhow::ensure!(
            max_per_peer <= max_circuits,
            "per-peer circuit limit exceeds total limit"
        );
        anyhow::ensure!(
            max_secs <= u32::MAX as u64,
            "circuit duration exceeds protocol limit"
        );
        Ok(Self {
            max_circuits: usize::try_from(max_circuits)?,
            max_per_peer: usize::try_from(max_per_peer)?,
            max_bytes,
            max_secs,
        })
    }

    pub fn to_libp2p(&self) -> relay::Config {
        relay::Config {
            max_circuits: self.max_circuits,
            max_circuits_per_peer: self.max_per_peer,
            max_circuit_bytes: self.max_bytes,
            max_circuit_duration: Duration::from_secs(self.max_secs),
            // Replace the default one-refill-per-two-minutes limiter, retaining
            // bounded bursts and sustained limits for both identity and IP.
            circuit_src_rate_limiters: Vec::new(),
            ..Default::default()
        }
        .circuit_src_per_peer(NonZeroU32::new(60).unwrap(), Duration::from_secs(10))
        .circuit_src_per_ip(NonZeroU32::new(120).unwrap(), Duration::from_secs(5))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_allow_complete_frames_and_keep_rate_limits() {
        let cfg = RelayLimits::parse(|_| None).unwrap().to_libp2p();
        assert!(cfg.max_circuit_bytes >= 3 * 4 * 1024 * 1024);
        assert_eq!(cfg.max_circuits, 64);
        assert_eq!(cfg.max_circuits_per_peer, 8);
        assert_eq!(cfg.max_circuit_duration, Duration::from_secs(900));
        assert_eq!(cfg.circuit_src_rate_limiters.len(), 2);
        assert!(!cfg.reservation_rate_limiters.is_empty());
    }

    #[test]
    fn invalid_limits_are_rejected_instead_of_panicking_or_disabling_limits() {
        for value in ["0", "garbage", "4294967296"] {
            assert!(RelayLimits::parse(|key| {
                (key == "FEDI3_P2P_RELAY_MAX_CIRCUIT_SECS").then(|| value.to_string())
            })
            .is_err());
        }
        assert!(RelayLimits::parse(|key| {
            (key == "FEDI3_P2P_RELAY_MAX_CIRCUITS_PER_PEER").then(|| "65".into())
        })
        .is_err());
    }
}
