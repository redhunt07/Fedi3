# syntax=docker/dockerfile:1

# Pinned toolchain image for reproducible builds.
FROM rust:1.99-bookworm AS builder
WORKDIR /src

# The workspace resolves every member manifest, so copy them all even though
# only the protocol and gateway sources are compiled.
COPY Cargo.toml Cargo.lock ./
COPY crates/fedi3_protocol/Cargo.toml crates/fedi3_protocol/Cargo.toml
COPY crates/fedi3_relay/Cargo.toml crates/fedi3_relay/Cargo.toml
COPY crates/fedi3_core/Cargo.toml crates/fedi3_core/Cargo.toml
COPY crates/fedi3_p2p_infra/Cargo.toml crates/fedi3_p2p_infra/Cargo.toml
COPY crates/fedi3_push_gateway/Cargo.toml crates/fedi3_push_gateway/Cargo.toml

# Cargo parses every workspace member manifest, so their source trees must be
# present even though only the protocol and gateway are compiled.
COPY crates/fedi3_protocol crates/fedi3_protocol
COPY crates/fedi3_push_gateway crates/fedi3_push_gateway
COPY crates/fedi3_core crates/fedi3_core
COPY crates/fedi3_relay crates/fedi3_relay
COPY crates/fedi3_p2p_infra crates/fedi3_p2p_infra

RUN cargo build -p fedi3_push_gateway --release --locked \
  && strip target/release/fedi3_push_gateway

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends ca-certificates curl \
  && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 10001 fedi3
COPY --from=builder /src/target/release/fedi3_push_gateway /usr/local/bin/fedi3_push_gateway

ENV FEDI3_PUSH_GATEWAY_BIND=0.0.0.0:8090
EXPOSE 8090

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD curl -fsS http://127.0.0.1:8090/healthz || exit 1

USER 10001:10001
ENTRYPOINT ["/usr/local/bin/fedi3_push_gateway"]
