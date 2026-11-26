# Persistence (v0.0.2)

Waves 53–75 replaced the v0.0.1 "pool connects, handlers don't
persist" limit with a working record store.

## How it works

- With `DATABASE_URL` set, the API applies migrations 0001–0005 at
  startup. Migration 0005 creates `mp_records (collection, id, data
  JSONB)` — one row per record.
- On startup the API **hydrates** its in-memory collections from
  `mp_records` before serving. A background task **snapshots** the
  full state back every `PERSIST_SNAPSHOT_SECS` seconds (default
  15; `0` disables the task). Each snapshot is one transaction per
  run: a collection's rows are replaced atomically.
- All 22 collections are covered: users, merchants, api_keys,
  gateway_configs, settings, transactions, payment_links, invoices,
  webhook_events, deliveries, plans, subscriptions, refunds,
  settlements, disputes, notifications, audit_log, used_trx,
  risk_assessments, kyc_profiles, merchant_settings,
  refresh_tokens.
- User records persist their `password_hash` and `totp_secret`
  through a persistence-only representation — the API-facing
  serialization still never exposes them.
- Without `DATABASE_URL` nothing changes from v0.0.1: in-memory
  mode, data lost on restart, `/health` reports `"db":"in_memory"`.

## Observability

`GET /health` adds `persisted_collections` and `persisted_records`
(row counts in `mp_records`) when Postgres is live, `null` in
in-memory mode.

## Crash window

A hard kill can lose at most one snapshot interval (default 15 s)
of writes. Lower `PERSIST_SNAPSHOT_SECS` to shrink the window.

## Verification

`crates/api/tests/persistence_pg.rs` runs against a real Postgres
whenever `TEST_DATABASE_URL` is set (CI always sets it): a
snapshot/hydrate round-trip (including password-hash survival) and
a full HTTP register → merchant flow that survives a simulated
restart via hydration.
