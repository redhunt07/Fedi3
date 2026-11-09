use anyhow::{Context, Result};
use libp2p::{relay, Multiaddr, PeerId};
use std::{collections::HashMap, hash::Hash, net::IpAddr, num::NonZeroU32, time::Duration};
use web_time::Instant;

/// A circuit carries complete Fedi3 frames, not just a hole-punch handshake.
/// libp2p's 128 KiB / 2 minute defaults interrupt ordinary chat and sync traffic.
#[derive(Debug, Clone)]
pub struct RelayLimits {
    max_circuits: usize,
    max_per_peer: usize,
    max_bytes: u64,
    max_secs: u64,
    max_reservations_per_peer: usize,
}

/// Token bucket keyed by peer or IP address.
///
/// libp2p's default reservation limiters refill one token every two minutes,
/// which starves clients that retry a reservation after a transient relay
/// restart. The bucket here refills continuously so ordinary retries and
/// renewals are accepted while still bounding request bursts.
#[derive(Debug)]
struct ReservationBucket<K> {
    capacity: u32,
    refill_per_sec: u32,
    buckets: HashMap<K, (Instant, u32)>,
}

impl<K: Eq + Hash + Clone> ReservationBucket<K> {
    fn new(capacity: u32, refill_per_sec: u32) -> Self {
        Self {
            capacity,
            refill_per_sec,
            buckets: HashMap::new(),
        }
    }

    fn allow(&mut self, key: K, now: Instant) -> bool {
        let entry = self.buckets.entry(key).or_insert((now, self.capacity));
        let elapsed = now.saturating_duration_since(entry.0).as_secs();
        if elapsed > 0 {
            let added = elapsed.saturating_mul(u64::from(self.refill_per_sec));
            entry.1 = u32::try_from(u64::from(entry.1).saturating_add(added))
                .unwrap_or(u32::MAX)
                .min(self.capacity);
            entry.0 = now;
        }
        if entry.1 == 0 {
            return false;
        }
        entry.1 -= 1;
        true
    }
}

impl relay::RateLimiter for ReservationBucket<PeerId> {
    fn try_next(&mut self, peer: PeerId, _addr: &Multiaddr, now: Instant) -> bool {
        self.allow(peer, now)
    }
}

impl relay::RateLimiter for ReservationBucket<IpAddr> {
    fn try_next(&mut self, _peer: PeerId, addr: &Multiaddr, now: Instant) -> bool {
        let ip = addr.iter().find_map(|p| match p {
            libp2p::multiaddr::Protocol::Ip4(ip) => Some(IpAddr::V4(ip)),
            libp2p::multiaddr::Protocol::Ip6(ip) => Some(IpAddr::V6(ip)),
            _ => None,
        });
        match ip {
            Some(ip) => self.allow(ip, now),
            // A DNS address cannot be rate limited by IP; never reject it here.
            None => true,
        }
    }
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
        // A client can retry a closed reservation several times while a relay
        // restarts. Keep room for those retries without letting one peer pin
        // an unbounded number of reservations.
        let max_reservations_per_peer = number("FEDI3_P2P_RELAY_MAX_RESERVATIONS_PER_PEER", 8)?;
        anyhow::ensure!(
            max_per_peer <= max_circuits,
            "per-peer circuit limit exceeds total limit"
        );
        anyhow::ensure!(
            max_reservations_per_peer <= 128,
            "per-peer reservation limit exceeds the relay total"
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
            max_reservations_per_peer: usize::try_from(max_reservations_per_peer)?,
        })
    }

    pub fn to_libp2p(&self) -> relay::Config {
        relay::Config {
            max_circuits: self.max_circuits,
            max_circuits_per_peer: self.max_per_peer,
            max_circuit_bytes: self.max_bytes,
            max_circuit_duration: Duration::from_secs(self.max_secs),
            max_reservations_per_peer: self.max_reservations_per_peer,
            // One reservation per second with a burst of 120 per peer, and two
            // per second with a burst of 300 per IP. Retries after a relay
            // restart and periodic renewals stay accepted.
            reservation_rate_limiters: vec![
                Box::new(ReservationBucket::<PeerId>::new(120, 1)),
                Box::new(ReservationBucket::<IpAddr>::new(300, 2)),
            ],
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
        assert_eq!(cfg.max_reservations_per_peer, 8);
        assert_eq!(cfg.max_circuit_duration, Duration::from_secs(900));
        assert_eq!(cfg.circuit_src_rate_limiters.len(), 2);
        assert_eq!(cfg.reservation_rate_limiters.len(), 2);
    }

    #[test]
    fn reservation_limits_are_configurable_and_bounded() {
        let cfg = RelayLimits::parse(|key| {
            (key == "FEDI3_P2P_RELAY_MAX_RESERVATIONS_PER_PEER").then(|| "16".into())
        })
        .unwrap()
        .to_libp2p();
        assert_eq!(cfg.max_reservations_per_peer, 16);
        for value in ["0", "garbage", "129"] {
            assert!(RelayLimits::parse(|key| {
                (key == "FEDI3_P2P_RELAY_MAX_RESERVATIONS_PER_PEER").then(|| value.to_string())
            })
            .is_err());
        }
    }

    #[test]
    fn reservation_bucket_refills_over_time() {
        let mut bucket = ReservationBucket::<PeerId>::new(2, 1);
        let now = Instant::now();
        let peer = PeerId::random();
        assert!(bucket.allow(peer, now));
        assert!(bucket.allow(peer, now));
        assert!(!bucket.allow(peer, now));
        let later = now + Duration::from_secs(1);
        assert!(bucket.allow(peer, later));
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
