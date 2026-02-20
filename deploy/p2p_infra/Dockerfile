FROM rust:1.86 AS builder
WORKDIR /app
COPY . .
RUN cargo build -p fedi3_p2p_infra --release

FROM debian:bookworm-slim
ARG P2P_UID=1000
ARG P2P_GID=1000
ENV P2P_UID=${P2P_UID}
ENV P2P_GID=${P2P_GID}
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates gosu \
    && rm -rf /var/lib/apt/lists/*
RUN set -e; \
    if ! getent group "${P2P_GID}" >/dev/null 2>&1; then groupadd -g "${P2P_GID}" appuser; fi; \
    if ! getent passwd "${P2P_UID}" >/dev/null 2>&1; then useradd -m -u "${P2P_UID}" -g "${P2P_GID}" appuser; fi; \
    mkdir -p /data; \
    chown "${P2P_UID}:${P2P_GID}" /data
WORKDIR /app
COPY --from=builder /app/target/release/fedi3_p2p_infra /usr/local/bin/fedi3_p2p_infra
EXPOSE 4001/tcp
EXPOSE 4001/udp
COPY deploy/p2p_infra/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh
ENTRYPOINT ["/entrypoint.sh"]
