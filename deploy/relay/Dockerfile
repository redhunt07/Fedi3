# syntax=docker/dockerfile:1

# Pinned toolchain image (not a floating `1` tag) for reproducible builds.
FROM rust:1.99-bookworm AS builder
WORKDIR /src

# Copy the lockfile too so `--locked` builds cannot silently drift.
COPY Cargo.toml Cargo.lock ./
COPY crates/fedi3_protocol/Cargo.toml crates/fedi3_protocol/Cargo.toml
COPY crates/fedi3_relay/Cargo.toml crates/fedi3_relay/Cargo.toml
COPY crates/fedi3_core/Cargo.toml crates/fedi3_core/Cargo.toml
COPY crates/fedi3_p2p_infra/Cargo.toml crates/fedi3_p2p_infra/Cargo.toml

# Copy sources
COPY crates/fedi3_protocol crates/fedi3_protocol
COPY crates/fedi3_relay crates/fedi3_relay
COPY crates/fedi3_core crates/fedi3_core
COPY crates/fedi3_p2p_infra crates/fedi3_p2p_infra

RUN cargo build -p fedi3_relay --release --locked \
  && strip target/release/fedi3_relay

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates gosu curl \
  && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 fedi3
WORKDIR /home/fedi3

COPY --from=builder /src/target/release/fedi3_relay /usr/local/bin/fedi3_relay
COPY deploy/relay/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV FEDI3_RELAY_BIND=0.0.0.0:8787
EXPOSE 8787

HEALTHCHECK --interval=30s --timeout=5s --start-period=20s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8787/healthz || exit 1

ENTRYPOINT ["/entrypoint.sh"]
