//! Validate DNS answers in the connector itself, including redirect destinations.
//! The connector consumes these exact addresses, avoiding a check/resolve race.
use reqwest::dns::{Addrs, Name, Resolve, Resolving};
use std::net::IpAddr;

pub fn is_public_ip(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let o = ip.octets();
            !(ip.is_private()
                || ip.is_loopback()
                || ip.is_link_local()
                || ip.is_broadcast()
                || ip.is_documentation()
                || ip.is_multicast()
                || o[0] == 0
                || o[0] >= 240
                || (o[0] == 100 && (64..=127).contains(&o[1]))
                || (o[0] == 198 && (18..=19).contains(&o[1])))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return is_public_ip(&IpAddr::V4(v4));
            }
            // Only global unicast; reject documentation and transition ranges.
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000
                && !(s[0] == 0x2001 && (s[1] == 0xdb8 || s[1] < 0x200))
                && s[0] != 0x2002
                && !(s[0] == 0x3fff && s[1] < 0x1000)
        }
    }
}

pub struct PublicDnsResolver;

impl Resolve for PublicDnsResolver {
    fn resolve(&self, name: Name) -> Resolving {
        Box::pin(async move {
            let addresses: Vec<_> = tokio::net::lookup_host((name.as_str(), 0)).await?.collect();
            if addresses.is_empty() || addresses.iter().any(|a| !is_public_ip(&a.ip())) {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "DNS target is not public",
                )
                .into());
            }
            Ok(Box::new(addresses.into_iter()) as Addrs)
        })
    }
}

/// Federation must not delegate destination resolution to an ambient HTTP proxy.
pub fn guard_client(
    builder: reqwest::ClientBuilder,
    allow_private: bool,
) -> reqwest::ClientBuilder {
    let builder = builder.no_proxy();
    if allow_private {
        builder
    } else {
        builder.dns_resolver(std::sync::Arc::new(PublicDnsResolver))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_ranges_are_not_public() {
        for ip in [
            "0.1.2.3",
            "127.0.0.1",
            "224.0.0.1",
            "240.0.0.1",
            "198.18.0.1",
            "100.64.0.1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
            "2002:7f00:1::",
            "64:ff9b::7f00:1",
            "3fff::1",
        ] {
            assert!(!is_public_ip(&ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "8.8.8.8", "2606:4700:4700::1111"] {
            assert!(is_public_ip(&ip.parse().unwrap()), "{ip}");
        }
    }

    #[tokio::test]
    async fn redirect_to_private_hostname_never_connects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let start = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = target.local_addr().unwrap().port();
        let address = start.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = start.accept().await.unwrap();
            let mut buf = [0; 4096];
            assert!(stream.read(&mut buf).await.unwrap() > 0);
            stream.write_all(format!("HTTP/1.1 302 Found\r\nLocation: http://localhost:{port}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        });
        // Test fixture overrides only the initial public hostname.
        let client = guard_client(reqwest::Client::builder(), false)
            .resolve("initial.example", address)
            .build()
            .unwrap();
        assert!(client
            .get(format!("http://initial.example:{}/", address.port()))
            .send()
            .await
            .is_err());
        server.await.unwrap();
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), target.accept())
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn connector_rejects_private_hostname() {
        assert!(PublicDnsResolver
            .resolve("localhost".parse().unwrap())
            .await
            .is_err());
    }
}
