# Changelog

All notable changes to MiraclePay. Versions follow the wave log
in `WAVES.md`; the API surface is specified by the OpenAPI
document the server itself serves (`GET /openapi.json`).

## [0.0.2] — unreleased

### Added
- **Runtime Postgres persistence** for all API state (28
  collections): snapshot + hydrate over the record store, with
  real-Postgres integration tests in CI. In-memory mode remains
  the no-`DATABASE_URL` fallback.
- **Live gateway calls** (sandbox by default) for bKash
  tokenized, Stripe Checkout, PayPal Orders and Razorpay Orders
  behind env credentials; signed webhook receivers for Stripe
  and PayPal and a server-verified bKash callback. Without
  credentials every provider answers NotConfigured — nothing is
  ever faked.
- **Plugin system** (see `docs/plugins.md`): psychoscript-based
  confined JS plugins + an HTTP plugin lane. Manifest
  (`plugin.toml`), host functions (allowlisted `http.fetch`,
  own-config, per-plugin KV, log, HMAC), hard runtime limits,
  admin management API + dashboard page, pilot plugins for
  manual bank transfer and BRAC Bank SMS parsing.
- **Auth completion**: password reset and email verification
  (one-time hashed tokens), session list/revoke, admin user
  suspend/activate.
- **Product depth**: merchant onboarding checklist, team
  members, API-key authentication with scope enforcement,
  dispute evidence files, payout schedules, transaction CSV
  export, dashboard search, gateway test-connection,
  payment-link analytics, receipt PDFs.
- **Production hardening**: SMTP STARTTLS, graceful shutdown
  with a final persistence snapshot, startup config validation
  (fail-fast in production), `/health/live` + `/health/ready`,
  backup/restore drill in CI, env-gated signed Android release
  builds.

### Changed
- App version 0.0.2 across the Rust workspace, frontend, JS SDK
  and the Android app (`versionCode 2`).
- Invoice/receipt PDFs now carry the correct version footer.

### Fixed
- Full AGPL-3.0 `LICENSE` at the repo root (the GitHub license
  API previously 404'd).
- Docker frontend image now takes `PUBLIC_API_URL` as a build
  argument wired through Compose — previously the bundle always
  pointed at the visitor's `localhost:8080`.

## [0.0.1] — 2026-11-26

First release: clean-room PipraPay-class payment automation —
Rust (Axum) API with 14 gateway adapters, SMS verification,
payment links, invoices, subscriptions, refunds, settlements,
disputes, webhooks with retries, TOTP 2FA, refresh-token
rotation, fraud signals, KYC, Astro dashboard (Bangla + English),
Java Android SMS forwarder. 678 Rust + 7 JS SDK tests, 89
OpenAPI operations. State was in-memory at runtime (Postgres
schema shipped); gateway adapters were spec/config only.
