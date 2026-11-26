# Production checklist (Wave 95)

Before pointing real traffic at MiraclePay:

## Configuration (validated at startup, Wave 93)
- [ ] `APP_ENV=production` set — invalid config now refuses to boot.
- [ ] `JWT_SECRET` set to a random value ≥ 32 chars (never the dev fallback).
- [ ] `DATABASE_URL` set (Postgres 16) — without it you are in
      in-memory mode and every restart wipes the platform.
- [ ] `ADMIN_EMAIL` / `ADMIN_PASSWORD` set on first boot only, then
      the password changed from the seeded value.
- [ ] `PERSIST_SNAPSHOT_SECS` understood (default 15 s crash window;
      clean shutdowns snapshot at exit, Wave 92).

## Payments
- [ ] Gateway credentials in the environment (see
      `docs/gateways-live.md` matrix); sandbox flags flipped to live
      deliberately, one gateway at a time.
- [ ] `STRIPE_WEBHOOK_SECRET` / `PAYPAL_WEBHOOK_ID` set and the
      webhook URLs registered in the provider dashboards:
      `/api/v1/webhooks/stripe`, `/api/v1/webhooks/paypal`,
      bKash callback `/api/v1/callbacks/bkash`.
- [ ] Android forwarder installed on the merchant phone, battery
      exemption granted, device key created, sender filters checked.

## Email
- [ ] `SMTP_HOST` + `SMTP_FROM` set together; `SMTP_TLS=starttls`
      for any public relay (credentials are refused in clear text).
- [ ] Test dispatch from Admin → Notifications.

## Android release signing (Wave 87)
- [ ] Repository secrets set to sign the release APK:
      `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`,
      `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. Without them CI
      ships the unsigned artifact — never install that on a
      merchant's phone as a "release".

## Operations
- [ ] Nightly `scripts/backup.sh` cron + off-host copy; quarterly
      restore drill (`scripts/restore-drill.sh` runs in CI).
- [ ] `/health/ready` wired to the load balancer / orchestrator;
      `/metrics` scraped.
- [ ] TLS termination in front of API + frontend; Postgres port
      never public.
