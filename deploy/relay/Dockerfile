# syntax=docker/dockerfile:1

FROM rust:1-bookworm AS builder
WORKDIR /src

# Pre-copy manifests for better caching
COPY Cargo.toml Cargo.toml
COPY crates/fedi3_protocol/Cargo.toml crates/fedi3_protocol/Cargo.toml
COPY crates/fedi3_relay/Cargo.toml crates/fedi3_relay/Cargo.toml
COPY crates/fedi3_core/Cargo.toml crates/fedi3_core/Cargo.toml
COPY crates/fedi3_p2p_infra/Cargo.toml crates/fedi3_p2p_infra/Cargo.toml

# Copy sources
COPY crates/fedi3_protocol crates/fedi3_protocol
COPY crates/fedi3_relay crates/fedi3_relay
COPY crates/fedi3_core crates/fedi3_core
COPY crates/fedi3_p2p_infra crates/fedi3_p2p_infra

RUN cargo build -p fedi3_relay --release

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates gosu \
  && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 fedi3
WORKDIR /home/fedi3

COPY --from=builder /src/target/release/fedi3_relay /usr/local/bin/fedi3_relay
COPY deploy/relay/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENV FEDI3_RELAY_BIND=0.0.0.0:8787
EXPOSE 8787

ENTRYPOINT ["/entrypoint.sh"]
