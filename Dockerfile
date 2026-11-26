# MiraclePay v0.0.1 — API + worker image (Wave 4)
# Multi-stage: build the Rust workspace binaries, ship them on a slim
# runtime with no toolchain. One image, two entrypoints:
#   api    → `miraclepay-api`    (default CMD)
#   worker → `miraclepay-worker` (compose overrides the command)

# v0.0.3.2: the api binary embeds the frontend bundle, so build
# the Astro site first and hand dist/ to the Rust build.
# PUBLIC_API_URL is emptied: the embedded UI calls the API
# relative to its own origin. (The separate frontend image in
# Dockerfile.frontend is unchanged for compose deployments.)
FROM node:24-alpine AS web
WORKDIR /web
COPY frontend/package.json frontend/package-lock.json ./
RUN npm ci
COPY frontend/ ./
ENV PUBLIC_API_URL=
RUN npm run build

FROM rust:1-slim-bookworm AS builder
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY migrations ./migrations
COPY --from=web /web/dist ./frontend/dist
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
