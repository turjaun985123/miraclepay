# MiraclePay v0.0.1 — API + worker image (Wave 4)
# Multi-stage: build the Rust workspace binaries, ship them on a slim
# runtime with no toolchain. One image, two entrypoints:
#   api    → `miraclepay-api`    (default CMD)
#   worker → `miraclepay-worker` (compose overrides the command)

FROM rust:1-slim-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
RUN cargo build --release -p miraclepay-api -p miraclepay-worker \
    && strip target/release/miraclepay-api target/release/miraclepay-worker

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --uid 10001 miraclepay
COPY --from=builder /src/target/release/miraclepay-api /usr/local/bin/miraclepay-api
COPY --from=builder /src/target/release/miraclepay-worker /usr/local/bin/miraclepay-worker
USER miraclepay
ENV PORT=8080
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --retries=3 \
    CMD curl -fsS http://127.0.0.1:8080/health || exit 1
CMD ["miraclepay-api"]
