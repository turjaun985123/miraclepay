# Operations runbook

Day-2 operations for a MiraclePay deployment. Everything here
uses surfaces the platform actually exposes — no direct database
surgery except where marked.

## Know your deployment in 30 seconds

```bash
curl -s https://your-host/health
# {"status":"ok","version":"0.0.1","name":"MiraclePay","db":"postgres","sandbox":false}
curl -s https://your-host/api/versions
curl -s https://your-host/metrics | head -40
```

- `db` is `postgres` when `DATABASE_URL` connected and migrations
  ran, otherwise `in_memory` — in-memory mode loses all state on
  restart and refuses pg_dump backups (by design, see
  backup-restore.md).
- `sandbox: true` on a public deployment is a **misconfiguration**:
  the demo login is public knowledge. Unset `SANDBOX_MODE` and
  restart against a fresh store.
- Logs are one JSON line per request on stdout
  (`request_id`, `method`, `path`, `status`, `latency_ms`). When a
  user reports an error, ask for the `x-request-id` response
  header and grep for it.

## Environment reference

Core: `PORT` (default 8080), `JWT_SECRET` (required in practice —
pick a long random value), `DATABASE_URL` (optional; unset =
in-memory), `ADMIN_EMAIL` + `ADMIN_PASSWORD` (seed the first admin
into an empty store), `CORS_ALLOWED_ORIGINS` (comma-separated;
empty = CORS off), `SANDBOX_MODE`.

Gateways (all optional; an unconfigured gateway answers
NotConfigured, never a fake success): `BKASH_USERNAME`,
`BKASH_PASSWORD`, `BKASH_APP_KEY`, `BKASH_APP_SECRET`,
`BKASH_SANDBOX`; `STRIPE_SECRET_KEY`; `PAYPAL_CLIENT_ID`,
`PAYPAL_CLIENT_SECRET`, `PAYPAL_SANDBOX`; `RAZORPAY_KEY_ID`,
`RAZORPAY_KEY_SECRET`. Manual-instruction gateways (Nagad, Rocket,
Upay, SureCash, UPI, Paytm, PhonePe, Easypaisa, JazzCash) are
configured per merchant in the dashboard.

Mail: `SMTP_HOST`, `SMTP_FROM` (both required to send),
`SMTP_PORT` (25), `SMTP_USER`/`SMTP_PASS`, `SMTP_HELO`.

## Incidents

### Webhooks piling up / dead letters
`miraclepay_webhook_deliveries_total{status="deadletter"}` on
/metrics is rising, or merchants report missing events.
Deliveries retry 8 times with exponential backoff, then
dead-letter. Fix the merchant endpoint first (their URL is on the
merchant record), then requeue from the dashboard's webhook page
or the redeliver endpoint (`POST /api/v1/webhooks/events/:id/redeliver`). Signing:
merchants verify with the v2 header (docs/webhooks.md).

### "SMTP not configured" on notification dispatch
`POST /api/v1/admin/notifications/dispatch` returns 409 naming
the missing variables. Set `SMTP_HOST` + `SMTP_FROM` and restart.
Queued notifications are never fake-sent; they wait.

### A merchant says payments are stuck Pending
Usually the SMS forwarder: ask the merchant to open the Android
app — the home screen shows device presence indirectly via the
dashboard API-keys page (`device_status`) and the app shows its
outbox ("Queue: N waiting · M failed"). Fixes: grant SMS
permission, set battery to Unrestricted, tap "Retry queued
messages". Server side, check the SMS corpus rules before blaming
the phone: a forwarded SMS that is a *failure notice* is rejected
by design.

### Rate-limit 429s
Budgets per client IP per minute: auth 10, SMS ingest 120, API
600. A merchant behind shared NAT can hit the API budget with a
busy integration — the answer is a conversation about their call
pattern, not a silent raise; budgets live in
`crates/api/src/rate_limit.rs`.

### Fraud flags
`GET /api/v1/admin/fraud/flagged` lists medium/high risk
payments. Flags are advisory (nothing auto-blocks); review and
use disputes (Wave 38 flow) when a chargeback actually arrives.

### Rollback
See upgrade-guide.md §Rollback. Short version: redeploy the
previous image; restore the pre-upgrade backup if the release
migrated the database.

## Routine

- Daily: backup (backup-restore.md), glance at /metrics
  dead-letter + failed-transaction counts.
- Weekly: review the admin audit log (`GET /api/v1/admin/audit`)
  and the KYC queue (`GET /api/v1/admin/kyc/pending`).
- On every deploy: /health `db` + `sandbox` fields, one sandbox
  payment end-to-end, one webhook delivery observed.
