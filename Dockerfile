FROM rust:1.91-slim-bookworm AS builder
RUN apt-get update && apt-get install -y --no-install-recommends gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY console ./console
RUN cargo build --release --locked --workspace

FROM docker:28-cli AS dockercli

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl nginx openssl procps \
    && rm -rf /var/lib/apt/lists/*
COPY --from=dockercli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=builder /build/target/release/phyxi-console /usr/local/bin/phyxi-console
ENV PHYXI_PORT=8765 PHYXI_ENABLE_HOST_SETUP=0 PHYXI_DOCKER_HOST=unix:///var/run/docker.sock
STOPSIGNAL SIGINT
ENTRYPOINT ["phyxi-console"]
