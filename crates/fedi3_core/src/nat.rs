use anyhow::{Context, Result};
use igd_next::{search_gateway, Gateway, PortMappingProtocol, SearchOptions};
use std::{
    net::{IpAddr, SocketAddr, UdpSocket},
    ops::RangeInclusive,
    time::Duration,
};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct NatPortMapping {
    pub internal_port: u16,
    pub external_port: u16,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct UpnpStatus {
    pub enabled: bool,
    pub external_ip: Option<IpAddr>,
    pub ports: Vec<u16>,
    pub description: String,
    pub last_error: Option<String>,
}

pub struct NatSession {
    gateway: Gateway,
    pub external_ip: IpAddr,
    pub lease_secs: u32,
    pub description: String,
    pub protocol: PortMappingProtocol,
    pub mapped_ports: Vec<NatPortMapping>,
}

impl NatSession {
    pub async fn discover(timeout: Duration, bind_ip: IpAddr) -> Result<Self> {
        let opts = SearchOptions {
            bind_addr: SocketAddr::new(bind_ip, 0),
            timeout: Some(timeout),
            ..Default::default()
        };
        let gateway = tokio::task::spawn_blocking(move || search_gateway(opts))
            .await
            .context("search gateway aborted")?
            .context("search gateway failed")?;
        let ext_ip = gateway
            .get_external_ip()
            .context("get external ip from gateway")?;
        Ok(Self {
            gateway,
            external_ip: ext_ip,
            lease_secs: 0,
            description: String::new(),
            protocol: PortMappingProtocol::UDP,
            mapped_ports: Vec::new(),
        })
    }

    pub async fn map_range(
        &mut self,
        range: RangeInclusive<u16>,
        lease_secs: u32,
        description: &str,
        protocol: PortMappingProtocol,
        local_ip: IpAddr,
    ) -> Result<()> {
        let mut mapped = Vec::new();
        for port in range {
            let addr = SocketAddr::new(local_ip, port);
            tokio::task::spawn_blocking({
                let gateway = self.gateway.clone();
                let desc = description.to_string();
                move || gateway.add_port(protocol, port, addr, lease_secs, desc.as_str())
            })
            .await??;
            mapped.push(NatPortMapping {
                internal_port: port,
                external_port: port,
            });
        }
        self.protocol = protocol;
        self.lease_secs = lease_secs;
        self.description = description.to_string();
        self.mapped_ports = mapped;
        Ok(())
    }

    pub async fn unmap_all(&mut self) -> Result<()> {
        for mapping in self.mapped_ports.drain(..) {
            let protocol = self.protocol;
            let port = mapping.external_port;
            tokio::task::spawn_blocking({
                let gateway = self.gateway.clone();
                move || gateway.remove_port(protocol, port)
            })
            .await??;
        }
        Ok(())
    }
}

pub fn default_local_ip() -> Result<IpAddr> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).context("bind socket for local ip")?;
    socket
        .connect(("8.8.8.8", 53))
        .context("connect to public resolver for local ip")?;
    Ok(socket.local_addr().context("local addr")?.ip())
}

pub struct UpnpController {
    session: Option<NatSession>,
    pub status: UpnpStatus,
    range: Option<RangeInclusive<u16>>,
    lease_secs: u32,
    timeout: Duration,
}

impl UpnpController {
    pub fn new(
        range: Option<RangeInclusive<u16>>,
        lease_secs: u32,
        description: String,
        timeout: Duration,
    ) -> Self {
        let (enabled, external_ip, ports) = (false, None, Vec::new());
        Self {
            session: None,
            range,
            lease_secs,
            timeout,
            status: UpnpStatus {
                enabled,
                external_ip,
                ports,
                description,
                last_error: None,
            },
        }
    }

    pub fn status(&self) -> UpnpStatus {
        self.status.clone()
    }

    pub async fn enable(&mut self) -> Result<UpnpStatus> {
        if self.status.enabled {
            return Ok(self.status.clone());
        }
        let range = match self.range.clone() {
            Some(r) => r,
            None => {
                self.status.last_error = Some("port range not configured".to_string());
                return Ok(self.status.clone());
            }
        };
        let local_ip = default_local_ip().context("determine local ip")?;
        let mut last_err: Option<anyhow::Error> = None;
        let max_attempts = 3u32;
        for attempt in 1..=max_attempts {
            match NatSession::discover(self.timeout, local_ip).await {
                Ok(mut session) => {
                    let mapped = session
                        .map_range(
                            range.clone(),
                            self.lease_secs,
                            &self.status.description,
                            PortMappingProtocol::UDP,
                            local_ip,
                        )
                        .await;
                    if let Err(e) = mapped {
                        last_err = Some(e);
                    } else {
                        self.status.enabled = true;
                        self.status.external_ip = Some(session.external_ip);
                        self.status.ports = session
                            .mapped_ports
                            .iter()
                            .map(|m| m.external_port)
                            .collect();
                        self.status.last_error = None;
                        self.session = Some(session);
                        return Ok(self.status.clone());
                    }
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
            if attempt < max_attempts {
                warn!(
                    attempt,
                    "UPnP mapping attempt failed; retrying after backoff"
                );
                tokio::time::sleep(Duration::from_millis(500 * attempt as u64)).await;
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("upnp mapping failed")))
    }

    pub async fn disable(&mut self) -> Result<UpnpStatus> {
        if let Some(mut session) = self.session.take() {
            session.unmap_all().await.ok();
        }
        self.status.enabled = false;
        self.status.external_ip = None;
        self.status.ports.clear();
        Ok(self.status.clone())
    }

    pub fn record_error(&mut self, err: String) {
        self.status.last_error = Some(err);
    }
}
