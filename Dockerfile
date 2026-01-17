# syntax=docker/dockerfile:1

FROM rust:1.75-slim AS builder

LABEL org.opencontainers.image.title="aigen-node" \
      org.opencontainers.image.description="AIGEN blockchain node" \
      org.opencontainers.image.licenses="UNLICENSED"

ENV DEBIAN_FRONTEND=noninteractive \
    CARGO_HOME=/cargo \
    RUSTFLAGS="-C target-cpu=native"

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY blockchain-core ./blockchain-core
COPY consensus ./consensus
COPY genesis ./genesis
COPY network ./network
COPY node ./node

RUN cargo build --release --bin node

FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.title="aigen-node" \
      org.opencontainers.image.description="AIGEN blockchain node" \
      org.opencontainers.image.licenses="UNLICENSED"

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
    && rm -rf /var/lib/apt/lists/*

RUN useradd -m -u 1000 -s /bin/bash aigen

COPY --from=builder /app/target/release/node /usr/local/bin/aigen-node
RUN chmod +x /usr/local/bin/aigen-node \
    && mkdir -p /data /config \
    && chown -R aigen:aigen /data /config

USER aigen
WORKDIR /data

EXPOSE 9000/tcp
EXPOSE 9944/tcp

ENTRYPOINT ["aigen-node"]
CMD ["start", "--config", "/config/config.toml"]

HEALTHCHECK --interval=30s --timeout=10s --start-period=60s --retries=3 \
  CMD curl -fsS -X POST http://127.0.0.1:9944 \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","method":"health","params":[],"id":1}' \
    | grep -q '"status":"healthy"'
