# Upgrade & migration guide

How to move a MiraclePay deployment between releases without
losing state or surprising API clients. v0.0.1 is the first
release; this guide defines the process every later release
follows.

## Compatibility promises

- **API:** v1 is additive-only (docs/api-versioning.md). An
  upgrade never requires client changes within v1.
- **Database:** migrations are append-only SQLx migrations
  (`migrations/0001_core.sql` … `0004_ops.sql` at v0.0.1). A
  migration, once released, is never edited — fixes ship as a new
  numbered migration. The API runs pending migrations on startup
  when `DATABASE_URL` is set.
- **In-memory mode:** there is nothing to migrate — and nothing
  survives a restart. Production means Postgres.

## Before you upgrade

1. Read the release notes for **new environment variables** and
   changed defaults. (v0.0.1's full env list is in runbook.md.)
2. Take a backup and verify it lists a size and a commit id
   (backup-restore.md). Do not skip this: rollback across a
   migration needs it.
3. Note the current image/tag and the current commit from
   `/health` (`version`) so rollback is a redeploy, not an
   archaeology project.

## Upgrade (Docker Compose)

```bash
git pull            # or set the new image tag in compose
docker compose build
docker compose up -d
curl -s localhost:8080/health   # expect status ok, db postgres, sandbox false
```

Watch the first minutes: request-log error rate, one real
checkout end-to-end, one webhook delivery.

## Rollback

- **No migration ran** (patch releases): redeploy the previous
  image. Done.
- **A migration ran:** redeploy the previous image *and* restore
  the pre-upgrade backup into a fresh database
  (`scripts/restore.sh`), then point `DATABASE_URL` at it. Never
  hand-edit migration bookkeeping tables to fake a downgrade.

## Configuration changes by area (v0.0.1 baseline)

| Area | Variables | Notes |
| --- | --- | --- |
| Core | `PORT`, `JWT_SECRET`, `DATABASE_URL`, `ADMIN_EMAIL`, `ADMIN_PASSWORD` | Admin seed only applies to an empty user store |
| CORS | `CORS_ALLOWED_ORIGINS` | Unset = no cross-origin browser access |
| Sandbox | `SANDBOX_MODE` | Never on a public deployment |
| Mail | `SMTP_HOST`, `SMTP_PORT`, `SMTP_FROM`, `SMTP_USER`, `SMTP_PASS`, `SMTP_HELO` | Host+From required to send |
| bKash | `BKASH_USERNAME`, `BKASH_PASSWORD`, `BKASH_APP_KEY`, `BKASH_APP_SECRET`, `BKASH_SANDBOX` | Tokenized checkout spec |
| Stripe | `STRIPE_SECRET_KEY` | Must be an `sk_` key |
| PayPal | `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET`, `PAYPAL_SANDBOX` | Sandbox defaults on |
| Razorpay | `RAZORPAY_KEY_ID`, `RAZORPAY_KEY_SECRET` | `rzp_test_`/`rzp_live_` prefixes validated |

Platform behaviour that is *not* env-driven lives in platform
settings (admin API): platform name, default currency/fee,
webhook max attempts, checkout session minutes, maintenance mode,
support email, and the FX rate table (Wave 34).
