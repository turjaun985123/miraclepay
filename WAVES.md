# MiraclePay — 50-Wave Roadmap & Log

Standing order (2026-11-26): "Don't stop until complete 50 waves."
Each wave = one coherent improvement → local tests → push → CI verified at
job + step level, confirmed twice → verdict logged here with commit + run id.

## Roadmap (50 waves)

| Wave | Scope |
|---|---|
| 1 | GitHub repo (public) + first CI verified (rust / frontend / android jobs) |
| 2 | Postgres/SQLx wiring — DATABASE_URL set = Postgres store, absent = in-memory fallback (clear log, never panic) |
| 3 | Argon2id password hashing + CSPRNG API keys/tokens; remove std-only placeholders |
| 4 | Root Dockerfile + working docker-compose (api + postgres + frontend) |
| 5 | Android build green in CI + release APK artifact uploaded |
| 6 | Webhook delivery persistence + retry/backoff in worker, dead-letter queue |
| 7 | bKash tokenized checkout (sandbox-shaped, env credentials, NotConfigured without) |
| 8 | Nagad + Rocket/Upay verification flows hardening |
| 9 | Stripe checkout-session adapter (env-gated) |
| 10 | PayPal adapter (env-gated) |
| 11 | Pagination + filtering completeness across list endpoints |
| 12 | Rate limiting (auth + API) |
| 13 | Audit-log UI in admin + dashboard |
| 14 | Invoice PDF generation |
| 15 | Payment-link expiry + one-time links |
| 16 | Subscription billing cycles (renewal worker, dunning) |
| 17 | Refunds flow end-to-end |
| 18 | Settlements / payouts flow end-to-end |
| 19 | Admin analytics dashboard (charts, ranges) |
| 20 | OpenAPI 3 spec for the full API |
| 21 | JS SDK package (@miraclepay/sdk) |
| 22 | Bangla (bn) i18n for frontend |
| 23 | Accessibility pass (a11y, keyboard, contrast) |
| 24 | Security headers + CORS hardening |
| 25 | Refresh tokens + session revocation |
| 26 | 2FA (TOTP) for admin accounts |
| 27 | Backup / restore docs + scripts |
| 28 | Load / performance pass (bench, indexes) |
| 29 | Easypaisa / JazzCash verification patterns hardening |
| 30 | UPI / Paytm / PhonePe / Razorpay patterns hardening |
| 31 | SMS pattern corpus + regression tests (BD/IN/PK) |
| 32 | Webhook signing v2 + event catalogue |
| 33 | Merchant onboarding / KYC-lite flow |
| 34 | Multi-currency groundwork (BDT/INR/PKR/USD display) |
| 35 | Email notifications (SMTP env-gated) |
| 36 | SMS device management UI (pair, revoke, heartbeat) |
| 37 | Checkout page branding per merchant |
| 38 | Disputes / chargeback record flow |
| 39 | Fraud signals (velocity, duplicate detection) |
| 40 | API versioning + deprecation headers |
| 41 | Frontend dashboard charts + realtime stats polling |
| 42 | Android: foreground-service reliability + Android 14 perms |
| 43 | Android: offline queue + retry UI |
| 44 | Observability: structured logs, /metrics, health depth |
| 45 | Error catalogue + consistent API error envelope |
| 46 | Seed/demo data + sandbox mode |
| 47 | Upgrade/migration guide + ops runbook |
| 48 | Full docs completeness pass (README, API, ARCHITECTURE) |
| 49 | Pre-release hardening: clippy -D warnings, fmt, audit deps |
| 50 | v0.0.1 release: tag v0.0.1 + GitHub Release with notes + APK |

## Log

(verdicts appended per wave below)

### Wave 1 — GitHub repo + first CI ✅
- Repo: https://github.com/turjaun985123/miraclepay (PUBLIC), commit `751711f`
- Secret scan before staging: clean (only dummy passwords in unit tests); 129 source files, no target/node_modules/dist/.env
- CI run **45750801468**: verified twice at job+step level — Frontend (Astro build) success, Android (assembleDebug) success (first Android build anywhere), Rust (test + build) success. All steps success.

### Wave 2 — Postgres/SQLx wiring ✅
- Commit `05384cd`, CI run **45751692615**: verified twice at job+step level — Rust success, Frontend success, Android success (0 failed steps).
- `crates/api/src/db.rs`: DATABASE_URL set → PgPool + `sqlx::migrate!` (migrations 0001–0004); unset/blank/unreachable → in-memory fallback with clear startup log, never panics. `AppState.db_pool` + `/health` now reports `"db": "in_memory"|"postgres"`.
- Local: 562 workspace tests pass (5 new db tests); live smoke: `/health` → `{"db":"in_memory",...}` with fallback log line.
- Scope note: handlers still read/write the in-memory collections in this wave; the pool is wired, migrations run, and mode is observable — per-handler SQLx queries migrate incrementally in later waves.

### Wave 3 — Argon2id passwords + CSPRNG API keys ✅
- Commit `476b855`, CI run **45752017248**: verified twice at job level — Rust success, Frontend success, Android success (0 failed steps).
- New `domain::password`: Argon2id PHC hashes, per-password CSPRNG salt; register/login/seed-admin use it. Legacy SHA-256 placeholder hashes explicitly do not verify (tested).
- `api_key::generate_key`: 16 bytes from OS CSPRNG (OsRng) — SplitMix64 time/counter derivation removed, key format unchanged.
- Local: 568 workspace tests pass (6 new password tests).
