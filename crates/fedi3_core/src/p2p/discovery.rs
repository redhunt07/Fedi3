//! Bounded peer cache and authenticated, application-specific peer exchange.
//! Contacts announce network endpoints only, never users, keys or chat contents.
use super::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub(super) const PRIVATE_DHT: &str = "/fedi3/kad/1";
pub(super) const EXCHANGE_PROTOCOL: &str = "/fedi3/peers/1";
const CONTACT_TTL_MS: i64 = 24 * 60 * 60 * 1000;
const CACHE_TTL_MS: i64 = 7 * CONTACT_TTL_MS;
const MAX_PEERS: usize = 256;
pub(super) const MAX_CONTACTS: usize = 16;
const MAX_ADDRS: usize = 8;
const FRAME_LIMIT: usize = 64 * 1024;

/// Public IPFS bootstrap identities, as published by the IPFS Foundation.
/// DNSADDR is expanded by the libp2p DNS transport, not by a fetched peer list.
pub(super) const PUBLIC_BOOTSTRAPS: &[&str] = &[
    "/dnsaddr/sg1.bootstrap.libp2p.io/p2p/QmcZf59bWwK5XFi76CZX8cbJ4BhTzzA3gU1ZjYZcYW3dwt",
    "/dnsaddr/sv15.bootstrap.libp2p.io/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    "/dnsaddr/am6.bootstrap.libp2p.io/p2p/QmbLHAnMoJPWSCR5Zhtx6BHJX9KiKNN6tpvbUcqanj75Nb",
    "/dnsaddr/ny5.bootstrap.libp2p.io/p2p/QmQCU2EcMqAqQPR2i9bChDtGNJchTbq5TbXJJ16u19uLTa",
];

/// A valid multihash used as a provider rendezvous key. Only provider records
/// are written to the public DHT; Fedi3's custom records stay on PRIVATE_DHT.
pub(super) fn public_provider_key() -> kad::RecordKey {
    use sha2::{Digest, Sha256};
    let mut key = vec![0x12, 0x20]; // sha2-256 multihash
    key.extend_from_slice(&Sha256::digest(b"fedi3/network/providers/v1"));
    kad::RecordKey::new(&key)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Contact {
    peer_id: String,
    addrs: Vec<String>,
    issued_at_ms: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct SignedContact {
    contact: Contact,
    public_key: String,
    signature: String,
}

impl SignedContact {
    fn signing_bytes(contact: &Contact) -> Result<Vec<u8>> {
        let mut bytes = b"fedi3-peer-contact-v1\0".to_vec();
        bytes.extend(serde_json::to_vec(contact)?);
        Ok(bytes)
    }

    pub(super) fn create(key: &identity::Keypair, addrs: &[String], now: i64) -> Option<Self> {
        let peer = key.public().to_peer_id();
        let addrs = sanitize_addrs(peer, addrs, true);
        if addrs.is_empty() {
            return None;
        }
        let contact = Contact {
            peer_id: peer.to_string(),
            addrs,
            issued_at_ms: now,
        };
        let signature = key.sign(&Self::signing_bytes(&contact).ok()?).ok()?;
        Some(Self {
            contact,
            public_key: B64.encode(key.public().encode_protobuf()),
            signature: B64.encode(signature),
        })
    }

    pub(super) fn verify(&self, now: i64) -> Option<(PeerId, Vec<String>)> {
        if self.contact.issued_at_ms < now - CONTACT_TTL_MS
            || self.contact.issued_at_ms > now + 5 * 60 * 1000
            || self.contact.addrs.is_empty()
            || self.contact.addrs.len() > MAX_ADDRS
            || self.public_key.len() > 1024
            || self.signature.len() > 1024
        {
            return None;
        }
        let peer: PeerId = self.contact.peer_id.parse().ok()?;
        let public =
            identity::PublicKey::try_decode_protobuf(&B64.decode(&self.public_key).ok()?).ok()?;
        if public.to_peer_id() != peer
            || !public.verify(
                &Self::signing_bytes(&self.contact).ok()?,
                &B64.decode(&self.signature).ok()?,
            )
        {
            return None;
        }
        let addrs = sanitize_addrs(peer, &self.contact.addrs, true);
        if addrs.len() != self.contact.addrs.len() {
            return None;
        }
        Some((peer, addrs))
    }
}

/// Only share numeric public endpoints. DNS/private endpoints may be configured
/// locally or learned directly, but are never forwarded to unrelated peers.
fn public_endpoint(addr: &Multiaddr) -> bool {
    let mut host = false;
    for protocol in addr.iter() {
        match protocol {
            Protocol::Ip4(ip) => {
                let [a, b, c, _] = ip.octets();
                if ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_unspecified()
                    || ip.is_multicast()
                    || ip.is_broadcast()
                    || ip.is_documentation()
                    || a == 0
                    || a >= 240
                    || (a == 100 && (64..=127).contains(&b))
                    || (a == 198 && (b == 18 || b == 19))
                    || (a == 192 && b == 0 && c == 0)
                {
                    return false;
                }
                host = true;
            }
            Protocol::Ip6(ip) => {
                let first = ip.segments()[0];
                // Global unicast only; reject mapped IPv4 and documentation.
                if first & 0xe000 != 0x2000 || (first == 0x2001 && ip.segments()[1] == 0x0db8) {
                    return false;
                }
                host = true;
            }
            Protocol::Tcp(port) | Protocol::Udp(port) if port != 0 => {}
            Protocol::QuicV1 | Protocol::P2p(_) | Protocol::P2pCircuit => {}
            _ => return false,
        }
    }
    host
}

pub(super) fn sanitize_addrs(peer: PeerId, values: &[String], public_only: bool) -> Vec<String> {
    let mut out = Vec::new();
    for value in values.iter().take(32) {
        if value.len() > 512 {
            continue;
        }
        let Ok(mut addr) = value.parse::<Multiaddr>() else {
            continue;
        };
        if public_only && !public_endpoint(&addr) {
            continue;
        }
        if let Some(Protocol::P2p(last)) = addr.iter().last() {
            if last != peer {
                continue;
            }
        } else {
            if addr.iter().any(|p| matches!(p, Protocol::P2pCircuit)) {
                continue;
            }
            addr.push(Protocol::P2p(peer));
        }
        let value = addr.to_string();
        if !out.contains(&value) {
            out.push(value);
        }
        if out.len() == MAX_ADDRS {
            break;
        }
    }
    out.sort();
    out
}

#[derive(Clone, Serialize, Deserialize)]
struct KnownPeer {
    peer_id: String,
    addrs: Vec<String>,
    learned_at_ms: i64,
    last_success_ms: i64,
    #[serde(default)]
    contact: Option<SignedContact>,
}

pub(super) struct PeerBook {
    path: PathBuf,
    peers: HashMap<PeerId, KnownPeer>,
    attempts: HashMap<PeerId, (i64, u32)>,
    own_peer: PeerId,
}

impl PeerBook {
    pub(super) fn len(&self) -> usize {
        self.peers.len()
    }
    pub(super) fn load(dir: &Path, own_peer: PeerId, now: i64) -> Self {
        let path = dir.join("p2p_known_peers.json");
        let mut this = Self {
            path,
            peers: HashMap::new(),
            attempts: HashMap::new(),
            own_peer,
        };
        if std::fs::metadata(&this.path)
            .map(|m| m.len() <= 2 * 1024 * 1024)
            .unwrap_or(false)
        {
            if let Ok(bytes) = std::fs::read(&this.path) {
                if let Ok(peers) = serde_json::from_slice::<Vec<KnownPeer>>(&bytes) {
                    for mut entry in peers.into_iter().take(MAX_PEERS) {
                        let Ok(peer) = entry.peer_id.parse::<PeerId>() else {
                            continue;
                        };
                        if peer == own_peer
                            || entry.learned_at_ms < now - CACHE_TTL_MS
                            || entry.learned_at_ms > now + 300_000
                        {
                            continue;
                        }
                        entry.addrs = sanitize_addrs(peer, &entry.addrs, false);
                        entry.contact = entry.contact.filter(|c| c.verify(now).is_some());
                        if !entry.addrs.is_empty() {
                            this.peers.insert(peer, entry);
                        }
                    }
                }
            }
        }
        this
    }

    pub(super) fn learned(&mut self, peer: PeerId, addrs: Vec<String>, now: i64, success: bool) {
        if peer == self.own_peer {
            return;
        }
        let addrs = sanitize_addrs(peer, &addrs, false);
        if addrs.is_empty() {
            return;
        }
        if !self.peers.contains_key(&peer) && self.peers.len() >= MAX_PEERS {
            // An established Fedi3 node can replace an unverified introduction.
            if !success {
                return;
            }
            if let Some(old) = self
                .peers
                .iter()
                .min_by_key(|(_, p)| (p.last_success_ms, p.learned_at_ms))
                .map(|(id, _)| *id)
            {
                self.peers.remove(&old);
                self.attempts.remove(&old);
            }
        }
        let entry = self.peers.entry(peer).or_insert_with(|| KnownPeer {
            peer_id: peer.to_string(),
            addrs: Vec::new(),
            learned_at_ms: now,
            last_success_ms: 0,
            contact: None,
        });
        if success {
            entry.addrs = addrs;
        } else {
            for addr in addrs {
                if !entry.addrs.contains(&addr) && entry.addrs.len() < MAX_ADDRS {
                    entry.addrs.push(addr);
                }
            }
        }
        entry.learned_at_ms = now;
        if success {
            entry.last_success_ms = now;
            self.attempts.remove(&peer);
        }
    }

    pub(super) fn introduce(
        &mut self,
        contact: SignedContact,
        now: i64,
    ) -> Option<(PeerId, Vec<String>)> {
        let (peer, addrs) = contact.verify(now)?;
        if peer == self.own_peer {
            return None;
        }
        if self
            .peers
            .get(&peer)
            .and_then(|p| p.contact.as_ref())
            .map(|c| c.contact.issued_at_ms >= contact.contact.issued_at_ms)
            .unwrap_or(false)
        {
            return None;
        }
        self.learned(peer, addrs.clone(), now, false);
        let entry = self.peers.get_mut(&peer)?;
        entry.contact = Some(contact);
        Some((peer, addrs))
    }

    pub(super) fn contacts(&self, now: i64) -> Vec<SignedContact> {
        let mut peers: Vec<_> = self
            .peers
            .values()
            .filter(|p| p.last_success_ms >= now - CACHE_TTL_MS)
            .collect();
        peers.sort_by_key(|p| std::cmp::Reverse(p.last_success_ms));
        peers
            .into_iter()
            .filter_map(|p| p.contact.as_ref())
            .filter(|c| c.verify(now).is_some())
            .take(MAX_CONTACTS - 1)
            .cloned()
            .collect()
    }

    pub(super) fn candidates(
        &mut self,
        now: i64,
        connected: &HashSet<PeerId>,
    ) -> Vec<(PeerId, Vec<String>)> {
        self.peers
            .retain(|_, p| p.learned_at_ms >= now - CACHE_TTL_MS);
        self.attempts.retain(|id, _| self.peers.contains_key(id));
        let mut peers: Vec<_> = self
            .peers
            .iter()
            .filter(|(id, _)| !connected.contains(id))
            .collect();
        peers.sort_by_key(|(_, p)| std::cmp::Reverse(p.last_success_ms));
        let mut out = Vec::new();
        for (peer, entry) in peers {
            let (next, failures) = self.attempts.entry(*peer).or_insert((0, 0));
            if now < *next {
                continue;
            }
            *failures = (*failures + 1).min(8);
            *next = now + (15_000i64 * (1i64 << *failures)).min(30 * 60 * 1000);
            out.push((*peer, entry.addrs.clone()));
            if out.len() == 4 {
                break;
            }
        }
        out
    }

    pub(super) fn snapshot(&self) -> (PathBuf, Vec<u8>) {
        (
            self.path.clone(),
            serde_json::to_vec(&self.peers.values().collect::<Vec<_>>()).unwrap_or_default(),
        )
    }
}

pub(super) fn save_snapshot((path, bytes): (PathBuf, Vec<u8>)) -> Result<()> {
    use std::io::Write;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&tmp)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(tmp, path)?;
    Ok(())
}

#[derive(Clone, Default)]
pub(super) struct ExchangeCodec;

impl request_response::Codec for ExchangeCodec {
    type Protocol = libp2p::StreamProtocol;
    type Request = Vec<SignedContact>;
    type Response = Vec<SignedContact>;
    async fn read_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Request>
    where
        T: futures_util::AsyncRead + Unpin + Send,
    {
        read_contacts(io).await
    }
    async fn read_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
    ) -> std::io::Result<Self::Response>
    where
        T: futures_util::AsyncRead + Unpin + Send,
    {
        read_contacts(io).await
    }
    async fn write_request<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        value: Self::Request,
    ) -> std::io::Result<()>
    where
        T: futures_util::AsyncWrite + Unpin + Send,
    {
        write_contacts(io, value).await
    }
    async fn write_response<T>(
        &mut self,
        _: &Self::Protocol,
        io: &mut T,
        value: Self::Response,
    ) -> std::io::Result<()>
    where
        T: futures_util::AsyncWrite + Unpin + Send,
    {
        write_contacts(io, value).await
    }
}

async fn read_contacts<T: futures_util::AsyncRead + Unpin + Send>(
    io: &mut T,
) -> std::io::Result<Vec<SignedContact>> {
    let contacts: Vec<SignedContact> = super::read_json_with_limit(io, FRAME_LIMIT).await?;
    if contacts.len() > MAX_CONTACTS {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "too many contacts",
        ));
    }
    Ok(contacts)
}

async fn write_contacts<T: futures_util::AsyncWrite + Unpin + Send>(
    io: &mut T,
    contacts: Vec<SignedContact>,
) -> std::io::Result<()> {
    if contacts.len() > MAX_CONTACTS || serde_json::to_vec(&contacts)?.len() > FRAME_LIMIT {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "too many contacts",
        ));
    }
    super::write_len_prefixed_json(io, &contacts).await
}

pub(super) fn private_kad(peer: PeerId) -> kad::Behaviour<kad::store::MemoryStore> {
    let cfg = kad::Config::new(libp2p::StreamProtocol::new(PRIVATE_DHT));
    kad::Behaviour::with_config(peer, kad::store::MemoryStore::new(peer), cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signed(key: &identity::Keypair, now: i64) -> SignedContact {
        SignedContact::create(key, &["/ip4/8.8.8.8/tcp/4001".into()], now).unwrap()
    }

    #[test]
    fn contacts_bind_signature_peer_addresses_and_expiry() {
        let key = identity::Keypair::generate_ed25519();
        let now = 2 * CACHE_TTL_MS;
        let original = signed(&key, now);
        assert_eq!(original.verify(now).unwrap().0, key.public().to_peer_id());
        let mut altered = original.clone();
        altered.contact.addrs = vec!["/ip4/1.1.1.1/tcp/4001".into()];
        assert!(altered.verify(now).is_none());
        let mut altered = original.clone();
        altered.contact.peer_id = identity::Keypair::generate_ed25519()
            .public()
            .to_peer_id()
            .to_string();
        assert!(altered.verify(now).is_none());
        assert!(original.verify(now + CONTACT_TTL_MS + 1).is_none());
        assert!(original.verify(now - 300_001).is_none());
    }

    #[test]
    fn exchange_never_shares_private_dns_or_other_peers_endpoints() {
        let key = identity::Keypair::generate_ed25519();
        for addr in [
            "/ip4/127.0.0.1/tcp/4001",
            "/ip4/192.168.1.2/tcp/4001",
            "/ip4/100.64.0.1/tcp/4001",
            "/ip4/169.254.169.254/tcp/80",
            "/ip6/::ffff:127.0.0.1/tcp/4001",
            "/dns4/localhost/tcp/4001",
            "/ip4/8.8.8.8/tcp/0",
        ] {
            assert!(
                SignedContact::create(&key, &[addr.into()], 0).is_none(),
                "{addr}"
            );
        }
        let other = identity::Keypair::generate_ed25519().public().to_peer_id();
        assert!(
            SignedContact::create(&key, &[format!("/ip4/8.8.8.8/tcp/4001/p2p/{other}")], 0)
                .is_none()
        );
        let circuit = format!(
            "/ip4/8.8.8.8/tcp/4001/p2p/{other}/p2p-circuit/p2p/{}",
            key.public().to_peer_id()
        );
        assert!(SignedContact::create(&key, &[circuit], 0).is_some());
    }

    #[test]
    fn cache_survives_restart_and_retries_with_backoff_and_expiry() {
        let dir = crate::test_support::TempDir::new("peer-book");
        let own = identity::Keypair::generate_ed25519().public().to_peer_id();
        let peer = identity::Keypair::generate_ed25519().public().to_peer_id();
        let now = 2 * CACHE_TTL_MS;
        let mut book = PeerBook::load(dir.path(), own, now);
        book.learned(peer, vec!["/ip4/127.0.0.1/tcp/12345".into()], now, true);
        save_snapshot(book.snapshot()).unwrap();
        let mut loaded = PeerBook::load(dir.path(), own, now);
        assert_eq!(loaded.candidates(now, &HashSet::new())[0].0, peer);
        assert!(loaded.candidates(now + 1, &HashSet::new()).is_empty());
        assert_eq!(loaded.candidates(now + 30_001, &HashSet::new()).len(), 1);
        assert!(loaded
            .candidates(now + CACHE_TTL_MS + 1, &HashSet::new())
            .is_empty());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(dir.path().join("p2p_known_peers.json"))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn introduced_contacts_are_not_forwarded_until_peer_was_connected() {
        let dir = crate::test_support::TempDir::new("peer-exchange");
        let key = identity::Keypair::generate_ed25519();
        let own = identity::Keypair::generate_ed25519().public().to_peer_id();
        let now = 2 * CACHE_TTL_MS;
        let mut book = PeerBook::load(dir.path(), own, now);
        let (peer, addrs) = book.introduce(signed(&key, now), now).unwrap();
        assert!(book.contacts(now).is_empty());
        book.learned(peer, addrs, now, true);
        assert_eq!(book.contacts(now).len(), 1);
        assert!(book.introduce(signed(&key, now - 1), now).is_none());
    }

    #[tokio::test]
    async fn exchange_rejects_oversized_frames_before_allocation() {
        let mut input =
            futures_util::io::Cursor::new(((FRAME_LIMIT + 1) as u32).to_be_bytes().to_vec());
        assert_eq!(
            read_contacts(&mut input).await.unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    #[test]
    fn corrupt_cache_is_ignored_and_public_seeds_have_peer_ids() {
        let dir = crate::test_support::TempDir::new("peer-book-corrupt");
        std::fs::write(dir.path().join("p2p_known_peers.json"), "partial write").unwrap();
        let own = identity::Keypair::generate_ed25519().public().to_peer_id();
        assert!(PeerBook::load(dir.path(), own, 0).peers.is_empty());
        for seed in PUBLIC_BOOTSTRAPS {
            let addr: Multiaddr = seed.parse().unwrap();
            assert!(matches!(addr.iter().last(), Some(Protocol::P2p(_))));
        }
    }
}
