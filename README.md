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
| Background worker | Rust (webhook delivery, settlements, subscriptions, SMTP mail) | `crates/worker` |
| Web frontend | Astro 7.4.13 + TypeScript, static | `frontend/` |
| JS SDK | Zero-dependency ESM client + TypeScript types | `sdk/js` (`@miraclepay/sdk`) |
| Android SMS forwarder | Java, `com.miraclepay.app`, SDK 34 / min 23 | `android/` |
| Database schema | PostgreSQL migrations | `migrations/0001–0004` |

## Quick start (local)

```bash
# Try it with demo data (sandbox mode — local evaluation only):
SANDBOX_MODE=1 JWT_SECRET=dev cargo run -p miraclepay-api   # http://localhost:8080
#   demo login: demo@miraclepay.local / demo1234  (docs/operations/sandbox.md)

# Real run:
cargo run -p miraclepay-api            # http://localhost:8080  (PORT env to change)
cd frontend && npm install && npm run dev   # http://localhost:4321

# Or everything at once:
docker compose up -d --build           # Postgres + API + worker + frontend
```

Environment (see `.env.example` and `docs/operations/runbook.md`):
`JWT_SECRET`, `PORT`, `ADMIN_EMAIL` / `ADMIN_PASSWORD` (seeds an admin
user on first boot), `DATABASE_URL` (Postgres pool + migrations;
unset = in-memory state), gateway credentials, `SMTP_*`,
`CORS_ALLOWED_ORIGINS`, `SANDBOX_MODE`.

## How a payment verifies

1. Merchant creates a payment (dashboard, payment link, invoice or API) and
   the payer gets send-money instructions (or a hosted checkout page).
2. The payer sends money to the merchant's wallet number.
3. The wallet's confirmation SMS arrives on the merchant's Android phone;
   the MiraclePay app forwards it via `POST /api/v1/sms` with the
   `x-device-key` header (an API key from Dashboard → API Keys).
4. `sms_verify` parses the SMS and scores it against pending transactions
   (TrxID 100, amount 50, sender 30, receiver/gateway 20). Exact matches
   settle instantly; probable matches are flagged for review. A TrxID
   match with a contradicting amount never settles, and failure notices
   are rejected outright (Wave 8/31 hardening).
5. On completion the merchant's webhook receives an HMAC-SHA256-signed
   `payment.completed` event (v1 + timestamped v2 signatures); the
   worker retries delivery with backoff and dead-letters after
   8 attempts.

## Docs

- `docs/API.md` — REST API conventions (auth, money, errors); the
  canonical endpoint catalogue is the OpenAPI document at
  `GET /openapi.json` (89 operations)
- `docs/ARCHITECTURE.md` — design, modularity rules, data model, security notes
- `docs/webhooks.md` — delivery, retries, both signing schemes, event catalogue
- `docs/errors.md` — the error-code catalogue for the unified error envelope
- `docs/api-versioning.md` — the v1 compatibility policy
- `docs/operations/runbook.md` — day-2 operations + incident playbooks
- `docs/operations/upgrade-guide.md` — upgrades, migrations, rollback
- `docs/operations/backup-restore.md` — backup/restore + restore drill
- `docs/operations/performance.md` — load-test method + measured floors
- `docs/operations/sandbox.md` — sandbox mode + demo data
- `WAVES.md` — the 50-wave improvement log with per-wave CI verdicts
- `REPORT.md` — build report for v0.0.1 (agent program, tests, known limits)
- `AGENTS_LOG.md` — the 100-agent build log

## Tests

```bash
cargo test --workspace        # 678 tests, verified locally
cd sdk/js && npm test         # 7 SDK tests (stubbed fetch)
cd frontend && npm run build  # Astro build
```

CI (`.github/workflows/ci.yml`) runs four jobs on every push:
Rust (test + build), Frontend (Astro build + SDK tests), Docker
(API + frontend images) and Android (debug + release APK,
uploaded as the `miraclepay-apks` artifact).
