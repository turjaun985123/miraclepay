# MiraclePay v0.0.1

Self-hosted payment automation platform — a clean-room reimplementation of
PipraPay's feature set (see `NOTICE.md`), written from scratch in Rust,
Astro and Java. Licence: AGPL-3.0.

MiraclePay lets a merchant accept mobile-wallet payments (bKash, Nagad,
Rocket, Upay, SureCash, UPI, Paytm, PhonePe, Razorpay, Easypaisa, JazzCash,
Stripe, PayPal, manual transfer) and **auto-verify them from the payment
SMS** the wallet sends to the merchant's phone: the MiraclePay Android app
forwards the SMS to the server, the verification engine parses and matches
it (amount + sender + transaction id), the transaction completes, and a
signed webhook notifies the merchant's system.

## Components

| Part | Stack | Where |
|---|---|---|
| REST API | Rust, Axum + Tokio | `crates/api` (binary: `miraclepay-api`) |
| Domain core | Rust, framework-free | `crates/domain` |
| Gateway adapters | Rust, one trait-based adapter per gateway | `crates/gateways` |
| SMS verification engine | Rust | `crates/sms_verify` |
| Background worker | Rust (webhook delivery, settlements, notifications) | `crates/worker` |
| Web frontend | Astro 7.4.13 + TypeScript, static | `frontend/` |
| Android SMS forwarder | Java, `com.miraclepay.app`, SDK 34 / min 23 | `android/` |
| Database schema | PostgreSQL migrations | `migrations/0001–0004` |

## Quick start (local)

```bash
# Backend API (in-memory state in v0.0.1; Postgres schema ships as migrations)
cargo run -p miraclepay-api            # http://localhost:8080  (PORT env to change)
# Frontend
cd frontend && npm install && npm run dev   # http://localhost:4321
```

Environment (see `.env.example`): `JWT_SECRET`, `PORT`,
`ADMIN_EMAIL` / `ADMIN_PASSWORD` (seeds an admin user on first boot),
`DATABASE_URL`, `REDIS_URL` (reserved for the Postgres-backed mode).

## How a payment verifies

1. Merchant creates a payment (dashboard, payment link, invoice or API) and
   the payer gets send-money instructions (or a hosted checkout page).
2. The payer sends money to the merchant's wallet number.
3. The wallet's confirmation SMS arrives on the merchant's Android phone;
   the MiraclePay app forwards it via `POST /api/v1/sms` with the
   `x-device-key` header (an API key from Dashboard → API Keys).
4. `sms_verify` parses the SMS and scores it against pending transactions
   (TrxID 100, amount 50, sender 30, receiver/gateway 20). Exact matches
   settle instantly; probable matches are flagged for review.
5. On completion the merchant's webhook receives an HMAC-SHA256-signed
   `payment.completed` event; the worker retries delivery with backoff.

## Docs

- `docs/API.md` — full REST API reference (42 paths, 51 handlers + subscriptions)
- `docs/ARCHITECTURE.md` — design, modularity rules, data model, security notes
- `REPORT.md` — build report for v0.0.1 (agent program, tests, known limits)
- `AGENTS_LOG.md` — the 100-agent build log
- `docker-compose.yml` — Postgres/Redis (+ api/worker build placeholders)

## Tests

```bash
cargo test --workspace   # 557 tests (unit + integration), verified locally
cd frontend && npm run build
```

CI (`.github/workflows/ci.yml`) runs cargo test, the Astro build and the
Android `assembleDebug` on every push/PR.
