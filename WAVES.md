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

### Wave 4 — Dockerfile + docker-compose full stack ✅
- Commit `53164fd`, CI run **45752150459**: verified twice at job+step level — Rust success, Frontend success, Android success, **Docker success** (new job builds both images; 0 failed steps).
- Root `Dockerfile` (multi-stage, api+worker binaries, non-root, healthcheck), `Dockerfile.frontend` (Astro build → nginx), `.dockerignore`; compose now has 5 services (postgres, redis profile, api, worker with correct `miraclepay-worker` command, frontend) and the stale "Dockerfile does not exist" note is gone.
- Local: compose YAML validated (5 services); no Docker on this VM, so image builds are verified in CI.

### Wave 5 — Android release APK + CI artifacts ✅
- Commit `3ac7ddd`, CI run **45752388388**: verified twice at job+step level — all 4 jobs success (0 failed steps).
- Android job now also runs `assembleRelease` (unsigned, proves release packaging) and uploads `miraclepay-apks` artifact — confirmed present on the run: 9,088,472 bytes (debug + release APKs).

### Wave 6 — Webhook delivery persistence + dead-letter (this commit)
- New `worker::delivery_store::FileDeliveryStore`: JSON-file persistence (atomic tmp+rename), automatic dead-letter separation, `due(now)` scheduling query, dead-letter requeue. Retry/backoff policy itself already lived in domain (8 attempts, exponential backoff to 24h cap) and is reused unchanged.
- Local: 573 workspace tests pass (5 new store tests; one parallel-test temp-file race found and fixed during the wave).

### Wave 6 — CI verdict ✅
- Commit `2ca7cfe`, CI run **45752654856**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 7 — bKash tokenized checkout shaping (this commit)
- New `gateways::bkash_tokenized`: grant-token / create-payment / execute-payment request specs + response parsers for bKash's documented tokenized API (sandbox default, live opt-in), credentials only from env (`BKASH_USERNAME/PASSWORD/APP_KEY/APP_SECRET`), missing credentials → `NotConfigured` naming them. No HTTP performed and no URL fabricated — transport stays with the caller, so the flow is fully unit-tested (6 new tests; 121 gateway tests pass locally).

### Wave 7 — CI verdict ✅
- Commit `03620b7`, CI run **45752907294**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 8 — Nagad/Rocket/Upay verification hardening + REAL BUG FIX (this commit)
- New `sms_verify::bd_templates`: realistic Nagad/Rocket/Upay template tests end-to-end (parse all fields → Verified against matching pending payment; replay → Duplicate; promo → never settles).
- **Bug found by those tests and fixed:** the matcher scored a provider-TrxID match alone as Exact (100 pts), so a Tk 999 payment auto-verified against a Tk 1,250 SMS with the same TrxID. `match_transaction` now refuses Exact whenever an SMS amount present contradicts the transaction amount (falls through to Probable/NoMatch rules). The old test that encoded the unsafe behaviour was rewritten to assert the safe one.
- Local: 588 workspace tests pass (9 new template tests + matcher guard).

### Wave 8 — CI verdict ✅
- Commit `e0876b0`, CI run **45753217844**: verified twice at job+step level — all 4 jobs success (0 failed steps). The matcher amount-contradiction fix is green in CI.

### Wave 9 — Stripe Checkout Session shaping ✅
- Commit `f64ee91`, CI run **45753443736**: verified twice at job+step level — all 4 jobs success (0 failed steps).
- `gateways::stripe_checkout`: create-session request spec (form-encoded, Bearer, metadata/client_reference_id = payment reference) + response parser; `STRIPE_SECRET_KEY` env-gated, non-`sk_` keys rejected as InvalidConfig, no fabricated checkout URLs. 5 new tests.

### Wave 10 — PayPal Orders v2 shaping ✅
- Commit `a0e6e21`, CI run **45753717070**: verified twice at job+step level — all 4 jobs success (0 failed steps).
- `gateways::paypal_checkout`: OAuth token request (Basic, base64 implemented in-crate with known-vector tests) + create-order spec (CAPTURE, custom_id = reference, PayPal-Request-Id idempotency) + response parsers with payer-action link extraction; env-gated (`PAYPAL_CLIENT_ID/SECRET`, sandbox default). 5 new tests.

### Wave 11 — Pagination completeness (this commit)
- The `pagination` module existed but no handler used it. Added `pagination::apply_page` (no params → legacy behaviour unchanged) and wired `page`/`per_page` into transactions, invoices, payment-links, webhook events and webhook deliveries list endpoints. Local: api tests 77 pass (3 new apply_page tests).

### Wave 11 — CI verdict ✅
- Commit `aae84e5`, CI run **45753968581**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 12 — Rate limiting (this commit)
- New `api::rate_limit`: fixed-window limiter (pure clock-injected core, 3 unit tests) + Axum middleware on the whole router. Budgets per client (first X-Forwarded-For hop) per minute: auth 10, SMS ingest 120, general API 600; non-API paths unlimited. Over-budget → 429 JSON `{error:"rate_limited", class}`. Local: api tests 80 pass, HTTP integration tests unaffected.

### Wave 12 — CI verdict ✅
- Commit `d85605c`, CI run **45754172001**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 13 — Audit-log UI, done properly (this commit)
- Found while wiring: the admin page's audit table guessed field names (`actor`, `actor_id`, …) that the API never sends — the Actor column always rendered "—". The real field is `actor_user_id`.
- Backend: `GET /admin/audit` gains `action` (prefix), `entity_type`, `actor_user_id` filters + Wave 11 paging (legacy `limit` behaviour preserved).
- Frontend: typed `AuditEntry` + `listAudit()` in `api.ts` matching the domain shape; admin page uses it, renders `actor_user_id` (null → "system"), and gains action/entity filter controls.
- Local: api tests pass; Astro build passes (13 pages).

### Wave 13 — CI verdict ✅
- Commit `215a188`, CI run **45754423703**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 14 — Invoice PDF (this commit)
- New `domain::invoice_pdf`: dependency-free one-page PDF renderer (correct xref offsets, PDF string escaping, totals from the domain's own subtotal/discount/tax/total) with 4 unit tests including an xref-offset check.
- New endpoint `GET /api/v1/invoices/:id/pdf` (owner-scoped, `application/pdf`, attachment filename = invoice number).
- Local: 608 workspace tests pass.

### Wave 14 — CI verdict ✅
- Commit `ffd3841`, CI run **45754679303**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 15 — Payment-link expiry + one-time links (this commit)
- Gap found: the domain and the pay flow already enforced `expires_at`/`max_uses`, but `create_link` hardcoded `expires_at: None` — merchants could never set an expiry.
- `POST /payment-links` now accepts `expires_at` (future, RFC 3339) or `expires_in_hours` (> 0; explicit wins), validated by a tested `resolve_expiry` helper (4 tests); `max_uses: 0` is rejected and `max_uses: 1` gives a true one-time link. Frontend `CreateLinkData` typed for all three fields.

### Wave 15 — CI verdict ✅
- Commit `2219464`, CI run **45754954128**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 16 — Subscription billing cycles (this commit)
- New `worker::subscription_job::run_billing_cycle`: walks due subscriptions once per cycle with an injected `Charger` — success renews (trial → active, period advances by plan interval), failure marks PastDue, PastDue still failing after the 7-day grace expires the subscription, scheduled cancellations expire uncharged at period end. 6 policy tests.

### Wave 16 — CI verdict ✅
- Commit `43d0b3c`, CI run **45755270123**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Waves 17+18 — Refunds end-to-end + Settlements API (this commit, one CI run)
- **W17 Refunds:** refunds could be created but never settled, and creation alone flipped the transaction to Refunded. Now: `POST /refunds/:id/settle {outcome}` records the terminal verdict, transaction status is recomputed from *settled* refunds only (`recompute_tx_refund_status`), creation no longer flips status, webhook `refund.succeeded`/`refund.failed` + audit emitted.
- **W18 Settlements:** settlements existed only as a worker job. New API: `GET /settlements` (owner-scoped, paging), `POST /settlements/run` (worker `collect_candidates`+`build_settlements` over a period; earlier settlement periods excluded so a period never pays twice; fee defaults to platform setting), `POST /settlements/:id/pay` (worker `mark_paid` with payout reference). API crate now depends on the worker crate for the job engines.

### Waves 17+18 — CI verdict ✅
- Commit `0709c61`, CI run **45755583567**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 19 — Admin analytics (this commit)
- New `domain::analytics`: zero-filled daily series (counts + completed BDT volume, reporting-currency convention shared with `compute_stats`) and per-gateway breakdown; 2 aggregation tests.
- New endpoint `GET /api/v1/admin/analytics?days=30` (admin-only, days clamped 1..=90).
- Admin page gains an Analytics section: daily bars + per-gateway table. Local: 620 workspace tests pass, Astro build passes.

### Wave 19 — CI verdict ✅
- Commit `244c59a`, CI run **45755851506**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 20 — OpenAPI 3.1 spec (this commit)
- New `api::openapi` module generating the spec from a route table mirroring `build_router` 1:1 (66 operations across 54 paths, `:param` → `{param}`, bearer-JWT security scheme, standard error responses).
- Served live at `GET /openapi.json`; unit tests pin structure, per-path presence, path-parameter declaration, and the operation count as a drift guard — adding a route without a spec entry now fails tests. Local: 624 workspace tests pass.

### Wave 20 — CI verdict ✅
- Commit `8870b46`, CI run **45756152367**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 21 — JavaScript SDK package (this commit)
- New `sdk/js` package `@miraclepay/sdk` 0.0.1: zero-dependency ESM client (Node 18+/browser fetch), one method per API operation, JWT-or-API-key bearer auth, `MiraclePayError` carrying status + server message, hand-written TypeScript definitions, README.
- 6 `node:test` tests with a stubbed fetch (request shape, auth precedence, login token storage, error mapping, query encoding) — all pass locally; the Frontend CI job now runs the SDK suite on every push.
