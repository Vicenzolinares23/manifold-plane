# Multi-stage build for mp-daemon.
FROM rust:1.85-bookworm AS builder
WORKDIR /src
COPY Cargo.toml rust-toolchain.toml ./
COPY crates ./crates
RUN cargo build -p mp-daemon --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=builder /src/target/release/mp-daemon /usr/local/bin/mp-daemon
ENV MP_ENGINE_ADDR=0.0.0.0:8787
EXPOSE 8787
CMD ["mp-daemon"]
