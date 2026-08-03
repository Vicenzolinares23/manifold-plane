# syntax=docker/dockerfile:1
# Build manifold-planed. Static-ish musl binary so the runtime image can be
# distroless — an admission controller with a shell in it is an admission
# controller an attacker can pivot through.

FROM rust:1-slim AS build
WORKDIR /src

RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config musl-tools \
    && rm -rf /var/lib/apt/lists/*
RUN rustup target add x86_64-unknown-linux-musl

COPY Cargo.toml ./
COPY crates ./crates

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --target x86_64-unknown-linux-musl -p mp-daemon \
    && cp target/x86_64-unknown-linux-musl/release/manifold-planed /manifold-planed

FROM gcr.io/distroless/static-debian12:nonroot
COPY --from=build /manifold-planed /usr/local/bin/manifold-planed
USER nonroot:nonroot
EXPOSE 8443
ENTRYPOINT ["/usr/local/bin/manifold-planed"]
