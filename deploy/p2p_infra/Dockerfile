FROM rust:1.86 AS builder
WORKDIR /app
COPY . .
RUN cargo build -p fedi3_p2p_infra --release

FROM debian:bookworm-slim
ARG P2P_UID=1000
ARG P2P_GID=1000
RUN set -e; \
    if ! getent group "${P2P_GID}" >/dev/null 2>&1; then groupadd -g "${P2P_GID}" appuser; fi; \
    if ! getent passwd "${P2P_UID}" >/dev/null 2>&1; then useradd -m -u "${P2P_UID}" -g "${P2P_GID}" appuser; fi; \
    mkdir -p /data; \
    chown "${P2P_UID}:${P2P_GID}" /data
WORKDIR /app
COPY --from=builder /app/target/release/fedi3_p2p_infra /usr/local/bin/fedi3_p2p_infra
USER appuser
EXPOSE 4001/tcp
EXPOSE 4001/udp
ENTRYPOINT ["/usr/local/bin/fedi3_p2p_infra"]
