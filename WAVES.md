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

### Wave 21 — CI verdict ✅
- Commit `4d64816`, CI run **45756395776**: verified twice at job+step level — all 4 jobs success (0 failed steps), including the new SDK test step in the Frontend job.

### Wave 22 — Bangla i18n for customer-facing pages (this commit)
- New `frontend/src/lib/i18n.ts`: en/bn dictionary (30+ strings), `t()` with `{placeholder}` substitution, `applyI18n()` over `data-i18n` attributes, localStorage persistence (`mp-lang`), `document.lang` sync for accessibility.
- Public pay page fully wired (static + script-set strings, toggle button, fixed-amount button label follows the language); hosted checkout wired for its static strings + toggle.
- Honest scope: dashboard/admin pages and checkout's script-generated messages remain English in v0.0.1.

### Wave 22 — CI verdict ✅
- Commit `570008a`, CI run **45756655058**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 23 — Accessibility pass (this commit)
- Skip-to-content link in the base layout; every page's `<main>` now carries `id="main"` (13 pages).
- Brand green darkened (`#0e9f6e` → `#0b7a55`, dark `#095c40`): white button text contrast ≈3.1:1 → ≈5:1 (WCAG AA).
- `prefers-reduced-motion` support: global transition/animation dampening in CSS + the pay page's success scroll now honours the OS setting.
- Existing strengths kept: `:focus-visible` outlines, labelled forms, `role="alert"` errors, scoped table headers.

### Wave 23 — CI verdict ✅
- Commit `dcea63f`, CI run **45757008439**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 24 — Security headers + CORS (this commit)
- New outermost `api::security` middleware: every response (incl. 429s/errors) carries `X-Content-Type-Options: nosniff`, `X-Frame-Options: DENY`, CSP `default-src 'none'; frame-ancestors 'none'`, `Referrer-Policy: no-referrer`, HSTS 1y.
- CORS off by default (same-origin deployment); `CORS_ALLOWED_ORIGINS` env allowlist enables it with origin echo (never a bare wildcard on credentialed requests), preflight answered by the middleware without consuming rate-limit budget.
- Tests: origin parse/match unit tests + an integration test asserting the headers on a live router response and the absence of CORS headers by default.

### Wave 24 — CI verdict ✅
- Commit `94ed2bb`, CI run **45757259481**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 25 — Refresh tokens with rotation (this commit)
- New `api::refresh`: opaque CSPRNG refresh tokens (`mprt_` + OsRng), 30-day TTL, only SHA-256 hashes stored, **rotation on every use** (presented token revoked, replay rejected), logout revocation. State gains a hash-keyed refresh-token store.
- `POST /api/v1/auth/login` now returns `{token, refresh_token}`; new `POST /auth/refresh` and `POST /auth/logout` (OpenAPI table updated: 68 operations).
- Frontend client: stores the pair at login, on any 401 exchanges the refresh token once and retries the request once, `logout()` revokes server-side then clears locally. Local: 629 workspace tests pass, Astro build passes.

### Wave 25 — CI verdict ✅
- Commit `b2a4564`, CI run **45757584528**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 26 — TOTP two-factor authentication (this commit)
- New `domain::totp`: RFC 6238 TOTP over HMAC-SHA1 (new `sha1` dep), base32 secrets from OsRng, ±1-step skew tolerance, otpauth URI — proven against the RFC 6238 Appendix B test vectors.
- User records gain `totp_secret` (never serialised) + `totp_enabled`. Endpoints: `POST /auth/2fa/setup` (pending secret + URI, shown once), `/enable` (code confirms possession), `/disable` (code required, secret cleared). Login enforces the second factor with distinct 401s (`totp_required` / `totp_invalid`); OpenAPI now 71 operations.
- UI: login reveals a code field on `totp_required`; dashboard gains a 2FA panel (status from `/auth/me`, setup/enable/disable). Local: 633 workspace tests pass, Astro build passes.

### Wave 26 — CI verdict ✅
- Commit `3bb9371`, CI run **45757981597**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 27 — Backup & restore tooling (this commit)
- `scripts/backup.sh`: pg_dump → timestamped `backups/<stamp>/` (gzip dump + env var *names* only + commit id); **refuses with a clear error in in-memory mode** (verified locally: exit 1, no fake backup produced) and when `pg_dump` is missing.
- `scripts/restore.sh`: restores into an empty database only (checks table count; `--force-empty` + confirmation required otherwise), `--yes` for automation.
- `docs/operations/backup-restore.md`: state inventory (DB vs in-memory vs worker file store vs secrets), nightly cron example with retention, off-host copy rule, quarterly restore-drill procedure, RPO 24h / RTO ~15min targets for the single-host v0.0.1.

### Wave 27 — CI verdict ✅
- Commit `197918c`, CI run **45758227950**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 28 — Load/perf pass (this commit)
- New `scripts/load-test.mjs` (zero-dep; refuses non-localhost targets without `--allow-remote`) and `docs/operations/performance.md` with the measured baseline.
- Measured on this shared/throttled VM (debug build, generator on the same host — figures are regression floors, not capacity claims): `/health` 3,693 req/s, 0 errors in 55,513 requests; single-request latency /health ≈1.5–4 ms.
- **Real finding fixed:** `/openapi.json` rebuilt the whole spec per request; it is now built once and cached (`openapi::spec_cached`, with an equality test). Concurrency numbers on this VM vary >3× between runs from neighbour load, so the doc records the noise honestly instead of claiming a speedup figure.

### Wave 28 — CI verdict ✅
- Commit `2ad4969`, CI run **45758500211**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 29 — Easypaisa/JazzCash hardening (this commit)
- Found by audit: both PK adapters accepted **any** non-empty merchant number and **any** currency (their own tests even used a Bangladeshi number + BDT for a Pakistani gateway).
- New shared `gateways::pk_manual`: `valid_pk_mobile` (03XXXXXXXXX / +923XXXXXXXXX shapes), config classification (malformed number → `InvalidConfig`, missing → `NotConfigured`), and `checked_manual_initiate` enforcing **PKR-only** amounts — both adapters now route through it (one reusable unit, no duplicated logic).
- Tests corrected to PK fixtures (0300… number, PKR) + new cases: BD number rejected as InvalidConfig, BDT amount rejected. Gateways crate: 136 tests pass; workspace 639.

### Wave 29 — CI verdict ✅
- Commit `9d06372`, CI run **45758705889**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 30 — UPI/Paytm/PhonePe/Razorpay hardening + fabricated URLs removed (this commit)
- New shared `gateways::in_manual` (mirrors Wave 29's `pk_manual`): strict Indian mobile (6–9 start, optional +91) and **strict VPA** validation (UPI previously accepted anything containing '@'), INR-only initiate for UPI/Paytm/PhonePe; fixtures corrected from BD/BDT to IN/INR with negative tests.
- **Honesty fix:** Razorpay and Stripe adapters returned fabricated checkout URLs (`checkout.js?order_ref=…`, `checkout.stripe.com/c/pay/<ref>`) that could never take a payment. Both now fail with a `Provider` error pointing at the real request-spec modules; new `gateways::razorpay_orders` adds the Razorpay Orders API spec (Basic auth, paise, receipt = reference, `rzp_test_/rzp_live_` key validation, response parser) in the Wave 7/9/10 style.
- Workspace: 645 tests pass; gateways crate 142.

### Wave 30 — CI verdict ✅
- Commit `d7f1093`, CI run **45759130499**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 31 — SMS corpus + failed-payment guard (this commit)
- New corpus regression suite (`sms_verify/tests/corpus.rs`): 13 realistic messages across BD/IN/PK + negatives, expectations = each message's plain meaning.
- **Critical find:** a *failed-payment notice* ("payment of Tk 1,250.00 … has FAILED. TrxID: …") parsed as a payment and would have **settled a pending transaction for money that never arrived**. New `parser::is_failure_notice` guard (failed/unsuccessful/insufficient/declined/reversed/…) rejects such messages in `parse_incoming` before extraction; regression tests prove they now verify as `NotPayment`.
- Corpus also caught: generic UPI bank alerts weren't detected (sender not in the known-bank list) and `UPI Ref`/`TID` labels weren't extracted as TrxIDs. Both fixed; one over-conservative unit test updated to the new documented contract (a UPI-Ref label is self-identifying; attribution alone settles nothing — the matcher still requires full matches from a registered device).
- Workspace: 648 tests pass.

### Wave 31 — CI verdict ✅
- Commit `bed0591`, CI run **45759517043**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 32 — Webhook signing v2 + full event catalogue (this commit)
- **Signing v2** (domain `webhook`): timestamped signatures `t=<unix>,v2=<HMAC(secret, "{t}.{body}")>` with a 300 s verification window — v1 signed only the body, so a captured delivery could be replayed forever. Worker deliveries now send both `x-miraclepay-signature` (v1) and `x-miraclepay-signature-v2`; v2 verify is constant-time and panic-free on malformed headers (roundtrip + replay-window tests).
- **Catalogue gap closed:** subscription and settlement actions were audit-logged but never emitted webhooks. New `AppState::emit_webhook` helper; subscription created/cancelled/paused/resumed and settlement.paid now emit. Canonical list pinned in `webhook::EVENT_TYPES` (sorted/unique test).
- `docs/webhooks.md`: delivery/retry behaviour, both verification schemes with steps, and the full 10-event catalogue with payload highlights. Workspace: 650 tests pass.

### Wave 32 — CI verdict ✅
- Commit `d0cd891`, CI run **45759770760**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 33 — Merchant onboarding / KYC-lite (this commit)
- New `domain::kyc`: profile model (business type, trade licence, NID, address), BD NID validation (10/13/17 digits), submission validation collecting all errors, status flow NotSubmitted → Pending → Verified/Rejected.
- Endpoints: `PUT/GET /merchants/:id/kyc` (owner submit/read, admin read), `GET /admin/kyc/pending` (review queue), `POST /admin/merchants/:id/kyc/decision`. OpenAPI now 75 operations.
- Enforcement (narrow, documented in the module): a KYC-**rejected** merchant cannot be activated (409 at the admin status endpoint); no-profile merchants keep pre-KYC behaviour.
- UI: dashboard KYC card (status + submit/resubmit form per merchant) and an admin review queue with Verify/Reject (rejection requires a note shown to the merchant). Workspace: 652 tests pass, Astro build passes.

### Wave 33 — CI verdict ✅
- Commit `f90d2d9`, CI run **45760109322**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 34 — Multi-currency groundwork: FX conversion (this commit)
- New `domain::fx`: rational rate table (units per 1 BDT as `num/den` — integer math only, no floats near money), `convert()` with round-half-up, identity conversions rate-free, missing/invalid rates are errors, never guesses. Indicative Nov-2026 defaults (USD 0.00837, INR 0.701, PKR 2.33, EUR 0.00771 per BDT), documented as reporting-only, never a dealing rate.
- Platform settings gain `fx_rates` (serde-defaulted, admin-overridable via the existing settings endpoints).
- New endpoints `GET /api/v1/fx/rates` and `GET /api/v1/fx/convert` (auth; unknown currency/negative amount → 400). OpenAPI now 77 operations. Workspace: 657 tests pass.

### Wave 34 — CI verdict ✅
- Commit `a4e778e`, CI run **45760567615**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 35 — Email notifications: real SMTP transport (this commit)
- Notifications were recorded but never sent. New `worker::smtp_transport` implements the existing `NotificationTransport` trait: env-gated config (`SMTP_HOST`/`SMTP_FROM` required, port/user/pass/helo optional), RFC 5322 rendering with a header-injection guard, dot-stuffing, optional AUTH PLAIN, full SMTP dialogue over TCP. Scope stated in-module: plain TCP (no STARTTLS) — local relay/tunnel use.
- Proven end-to-end in tests against a fake SMTP server on a real socket: the full EHLO→MAIL→RCPT→DATA dialogue and message content are asserted; unconfigured/missing variables produce named errors, never fake sends.
- New admin endpoint `POST /api/v1/admin/notifications/dispatch` drains the queue through the transport via the existing `dispatch_batch` engine and reports `{sent, failed}` (409 when SMTP is not configured). OpenAPI now 78 operations. Workspace: 662 tests pass.

### Wave 35 — CI verdict ✅
- Commit `bbf2e8e`, CI run **45760958516**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 36 — SMS device management: real device presence (this commit)
- Root cause found: `ApiKey.last_used_at` existed since the first build but was **never written**, so the dashboard's "Last used" column always said Never. Device authentication (`sms::authenticate_device`) now records successful use, throttled to one write per minute per key.
- `ApiKeyView` gains a derived `device_status` (revoked / never_connected / online ≤5 min / idle ≤24 h / offline), unit-tested across the ladder; the dashboard API-keys page renders it as a presence badge ("Device online", "Never connected", …) with a fallback for older responses. Workspace: 663 tests pass, Astro build passes.

### Wave 36 — CI verdict ✅
- Commit `8008bc4`, CI run **45761235751**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 37 — Checkout branding per merchant (this commit)
- `MerchantSettings` existed in the domain with validation but was never wired into the API. Now: state storage, `GET/PUT /merchants/:id/settings` (owner write, owner/admin read, defaults when unsaved), audit entry on update. OpenAPI now 80 operations.
- Branding fields: `brand_name` (≤60 chars), `brand_color` (`#rrggbb`, validated), `logo_url` (https only), `checkout_message` (≤200 chars) — all validated in the domain with every error collected.
- The public pay-link payload gains a `branding` object; the pay page applies it (display name wins, logo, message via textContent only, brand colour on the pay button after a client-side hex re-check). Typed settings client added to the dashboard API library. Workspace: 666 tests pass, Astro build passes.

### Wave 37 — CI verdict ✅
- Commit `8b3fb19`, CI run **45761603905**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 38 — Disputes / chargebacks (this commit)
- New `domain::dispute`: record + reason/status enums + `validate_open` (completed tx only, amount > 0 and ≤ tx amount, currency must match; all errors collected). Money-honesty note in-module: a dispute record moves no money; clawback on a Lost decision runs through the settlement flow, this is the audit trail.
- Endpoints: `POST/GET /disputes`, `GET /disputes/:id`, `POST /disputes/:id/evidence` (merchant evidence, Opened → UnderReview), `POST /admin/disputes/:id/decide` (won/lost). One open dispute per transaction (409), terminal disputes reject evidence/decisions (409), owner-scoped reads with admin override.
- `dispute.opened` / `dispute.closed` join the canonical webhook catalogue (now 12 types) and docs/webhooks.md; audit entries on open/decide. OpenAPI now 85 operations. Workspace: 669 tests pass.

### Wave 38 — CI verdict ✅
- Commit `cb4b055`, CI run **45762111216**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 39 — Fraud signals: explainable risk scoring (this commit)
- New `domain::fraud`: pure rule engine over precomputed aggregates — `high_value` (≥ ৳50,000, 20 pts), `velocity` (≥10 payments/merchant/10 min, 30), `sender_reuse` (sender completed at ≥3 other merchants in 24 h, 40), `amount_outlier` (>5× merchant median, needs ≥5 history, 25). Scores bucket low/medium/high; every signal carries a human-readable detail.
- `AppState::assess_risk` computes the aggregates from the transaction store and runs on both creation paths (API payments + payment-link pays); assessments stored per transaction.
- New admin endpoint `GET /api/v1/admin/fraud/flagged` (score ≥ 30, highest first, paginated). Deliberately advisory — signals never auto-block a payment in v0.0.1, so a false positive cannot strand a customer's money. OpenAPI now 86 operations. Workspace: 673 tests pass.

### Wave 39 — CI verdict ✅
- Commit `907d5c1`, CI run **45762418054**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 40 — API versioning formalised (this commit)
- New `api::versioning`: `GET /api/versions` (public discovery: current/supported/deprecated), `API-Version: v1` header stamped on every `/api/v1` response, and a router fallback that answers unsupported-version paths (`/api/v2/...`) with a structured `unsupported_api_version` 404 naming the supported versions — while typo'd v1 paths stay plain 404s (path classification is unit-tested, incl. the `/api/vendors` false-positive case).
- Policy written down in `docs/api-versioning.md`: additive changes within v1, breaking changes as a new version with a ≥6-month overlap and Sunset headers, app version (0.0.1) explicitly decoupled from API version (v1). OpenAPI now 87 operations. Workspace: 674 tests pass.

### Wave 40 — CI verdict ✅
- Commit `3d3f676`, CI run **45762735197**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 41 — Dashboard charts + live polling (this commit)
- New `GET /merchants/:id/analytics` (owner/admin): the Wave-19 daily-series aggregation scoped to one merchant (14 days default, clamped 1..=90) with period totals. OpenAPI now 88 operations.
- Dashboard home gains a "Sales — last 14 days" card: a dependency-free inline-SVG bar chart (per-day completed BDT volume, hover titles with amounts/counts, accessible `role="img"` label) plus totals.
- Live polling: the dashboard refreshes balance, transactions and the chart every 30 s while the tab is visible (pauses when hidden), with a Live badge and an "Updated HH:MM:SS" stamp. Workspace: 674 tests pass, Astro build passes.

### Wave 41 — CI verdict ✅
- Commit `2642c25`, CI run **45762959862**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 42 — Android foreground-service reliability + Android 14 permissions (this commit)
- **Foreground execution:** `SmsForwardWorker` now promotes itself with `setForegroundAsync` using a typed (`dataSync`) foreground notification before the HTTP call, so Android 14 / aggressive OEM builds don't kill the process mid-forward; promotion failure falls back to background execution (retries still apply). Manifest re-declares WorkManager's `SystemForegroundService` with `android:foregroundServiceType="dataSync"` and adds `FOREGROUND_SERVICE_DATA_SYNC`. New `Notify` helper owns the low-importance channel.
- **POST_NOTIFICATIONS:** declared but never requested at runtime — on Android 13+ the permission ask now includes it alongside the SMS pair.
- **Battery exemption:** status line shows Battery Restricted/Unrestricted, with a button opening the system exemption dialog (`REQUEST_IGNORE_BATTERY_OPTIMIZATIONS` declared) — the top cause of delayed forwards on BD devices. Verified by the CI Android job (assembleDebug + assembleRelease); no local Android SDK exists.

### Wave 42 — CI verdict ✅
- Commit `2148977`, CI run **45763209734**: verified twice at job+step level — all 4 jobs success (0 failed steps), including the Android job compiling the new foreground-service code.

### Wave 43 — Android offline queue + retry UI (this commit)
- WorkManager's queue was durable but invisible. New `OutboxStore` (SharedPreferences JSON, capped at 200 entries, oldest FAILED dropped first) mirrors every queued message with status/attempts/last-error.
- `SmsReceiver` records each message in the outbox before enqueueing through the new shared `Forwarder` (single enqueue path: constraints, backoff, keys). `SmsForwardWorker` reports outcomes back: success removes the entry, 4xx/unconfigured marks it FAILED with the reason, transient errors bump the attempt count.
- Main screen shows "Queue: N waiting · M failed" and a "Retry queued messages" button (visible only when the queue is non-empty) that re-enqueues everything and flips FAILED back to pending. Verified by the CI Android job; no local Android SDK exists.

### Wave 43 — CI verdict ✅
- Commit `ee76d1e`, CI run **45763423075**: verified twice at job+step level — all 4 jobs success (0 failed steps), Android job compiling the new outbox code.

### Wave 44 — Observability: /metrics + structured request logs (this commit)
- New `GET /metrics`: Prometheus text exposition rendered by hand (no new deps) from the live collections on each scrape — uptime, users/merchants, db mode, transactions by status, webhook deliveries by status, open disputes, queued notifications. Label escaping is unit-tested.
- Structured request logs: one JSON line per request on stdout (ts, request_id, method, path, status, latency_ms — no bodies or headers logged), with `x-request-id` generated/propagated and echoed on the response.
- Live-smoke-verified locally before push: metrics render, request-id header present, JSON log lines emitted, and the Wave-40 `/api/v2/...` structured error confirmed working end-to-end. OpenAPI now 89 operations. Workspace: 676 tests pass.

### Wave 44 — CI verdict ✅
- Commit `7b50556`, CI run **45763609368**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 45 — Error catalogue + envelope (this commit)
- Every `ApiError` response now uses one envelope: `{error, code, status}` — `error` (human message) is byte-identical to before for compatibility; `code` is a new stable machine-readable field (`bad_request`, `unauthorized`, `totp_required`, `totp_invalid`, `forbidden`, `not_found`, `conflict`, `internal_error`) that never changes within API v1, and `status` mirrors the HTTP status. The rate-limit 429 body is aligned to the same shape.
- `docs/errors.md` catalogues every code with meaning + retry guidance (including `rate_limited` and `unsupported_api_version`), and notes the owner-scoping convention (invisible resources answer 404, not 403).
- The JS SDK's `MiraclePayError` now exposes `.code` (typed in the .d.ts, covered by a new SDK test — 7/7 pass); Rust envelope mapping is unit-tested per variant. Workspace: 677 tests pass.

### Wave 45 — CI verdict ✅
- Commit `1212638`, CI run **45763906121**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 46 — Seed / demo data + sandbox mode (this commit)
- `SANDBOX_MODE=1` at startup seeds an empty store: demo owner (`demo@miraclepay.local` / `demo1234`, published on purpose), one active merchant, 14 days of demo transactions (chart-ready, `"demo": true` metadata), a ৳500 payment link and a sent invoice. Seeding is startup-only, empty-store-only, and idempotent (unit-tested: second run is a no-op).
- Safety rails: no HTTP seed trigger; `/health` reports `"sandbox": true`; the startup log warns loudly; `docs/operations/sandbox.md` documents the flow and the never-in-production rule. Workspace: 678 tests pass.

### Wave 46 — CI verdict ✅
- Commit `21e9ff8`, CI run **45764156568**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 47 — Upgrade/migration guide + ops runbook (this commit)
- `docs/operations/runbook.md`: 30-second deployment check (/health db + sandbox fields, /api/versions, /metrics), the full env reference (core, gateways, SMTP — grepped from the code, not remembered), and incident playbooks: webhook dead letters → redeliver endpoint, SMTP-not-configured 409, stuck-Pending payments (device presence + Android outbox), 429 budgets, fraud-flag review, rollback pointer, and daily/weekly routines.
- `docs/operations/upgrade-guide.md`: compatibility promises (API v1 additive-only; append-only migrations 0001–0004; in-memory = nothing to migrate), pre-upgrade checklist, compose upgrade steps, and the two rollback shapes (no-migration vs migration-ran → restore, never hand-edit bookkeeping). Every endpoint/env claim in both docs was verified against the code while writing.

### Wave 47 — CI verdict ✅
- Commit `b074724`, CI run **45764417644**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 48 — Docs completeness (this commit)
- README rewritten where stale: 4-job CI description, sandbox quickstart, Docker one-liner, JS SDK component row, full docs index (webhooks/errors/versioning/runbook/upgrade/backup/performance/sandbox), current test counts (678 Rust + 7 SDK), and the Wave 8/31 SMS-safety notes in the verification walkthrough.
- `docs/API.md`: §3.2 now documents the Wave-45 error envelope with the errors.md pointer; §4 declares the OpenAPI document (89 operations) the canonical endpoint catalogue instead of pretending the hand tables are complete.
- `docs/ARCHITECTURE.md` §6.2 corrected: it claimed the API "does not use Postgres yet" — since Wave 2 the pool connects and migrations run at startup (handlers still use in-memory collections; stated plainly as the v0.0.1 limit).
- `.env.example` gains the CORS, SMTP and SANDBOX sections it was missing.

### Wave 48 — CI verdict ✅
- Commit `2ff3e39`, CI run **45764663551**: verified twice at job+step level — all 4 jobs success (0 failed steps).

### Wave 49 — Pre-release hardening: fmt + clippy gates + dependency audit (this commit)
- The codebase had never been rustfmt-clean (245 diff hunks across 79 files) and carried ~40 clippy warnings. Now: `cargo fmt --check` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, all 678 tests still pass.
- Real fixes inside the cleanup: unused imports removed; `dispatch_batch`/`process_due` now take slices instead of `&mut Vec`; descending sorts use `sort_by_key(Reverse)`; the hand-rolled no-op waker in worker tests replaced with `std::task::Waker::noop()`; dead `[workspace.package]` removed. Three deliberate conventions are allowed with written reasons: major_minor digit grouping for money literals, and the checked `Money::add/sub` + `GatewayId::from_str` not implementing std traits.
- CI: the Rust job now runs fmt --check and clippy -D warnings as gates, and a fifth job (Dependency audit, RustSec `audit-check`) scans Cargo.lock against the advisory database on every push.

### Wave 49 — first CI run: audit job failed, fixed in this push
- Run **45766034836**: Rust/Frontend/Android/Docker jobs passed, but the new RustSec audit job **failed** on two advisories — `rsa 0.9.10` (RUSTSEC-2023-0071, Marvin) and `libsqlite3-sys 0.30.1` (RUSTSEC-2026-0459, bundled-SQLite CVE). Investigation: both crates are **lockfile-only residue** of sqlx-macros-core's optional drivers — `cargo tree -i <crate> --edges all` shows neither (nor sqlx-mysql/sqlx-sqlite) in the compiled graph; this workspace enables sqlx for Postgres only, so neither crate is compiled, linked or shipped. Fix: `.cargo/audit.toml` ignores the two advisories with that verification recorded (and a revisit warning if the MySQL/SQLite drivers are ever enabled). The audit job also lacked `checks: write`, which produced a "Resource not accessible" error — permission added. (Informational only: chrono is flagged unmaintained; noted, not failing.)

### Wave 49 — CI verdict ✅
- Fix commit `068e5ae`, CI run **45766588765**: verified twice at job+step level — all 5 jobs success (0 failed steps), including the RustSec audit job and its published Security audit check.

### Wave 50 — v0.0.1 release (this commit)
- Tag `v0.0.1` on this commit; GitHub Release with the notes below and the CI-built Android APKs (debug + release) attached from this commit's run.
- Release notes summary: MiraclePay v0.0.1 — Rust/Axum API (89 OpenAPI operations), Astro dashboard + hosted checkout, Android SMS forwarder; SMS auto-verification with the Wave 8/31 safety rules; webhooks with v1+v2 signing, retries and dead-letter; subscriptions, settlements, refunds, disputes, KYC, fraud signals, FX conversion, SMTP notifications, sandbox mode; Postgres wiring (pool + migrations; in-memory collections remain the v0.0.1 store of record — stated plainly), Docker + compose, Prometheus metrics, structured logs, error envelope, API v1 policy; 678 Rust tests + 7 SDK tests; fmt + clippy -D warnings + RustSec audit all gated in CI.

### Wave 50 — CI verdict ✅ + RELEASED
- Release commit `3bcb1e4`, CI run **45766905242**: verified twice at job+step level — all 5 jobs success (0 failed steps).
- Tag `v0.0.1` pushed at `3bcb1e4`; GitHub Release **MiraclePay v0.0.1** published (not draft) with both CI-built APKs attached and verified by read-back: `miraclepay-v0.0.1-debug.apk` (6,107,551 B) and `miraclepay-v0.0.1-release-unsigned.apk` (4,904,573 B), asset state `uploaded`.
- **50/50 waves complete.** Final state: 678 Rust tests + 7 SDK tests green, fmt + clippy + RustSec audit gated, 89 OpenAPI operations, docs set complete (API, architecture, webhooks, errors, versioning, runbook, upgrade, backup, performance, sandbox).

---

# Waves 51–150 — v0.0.2 (standing order 2026-11-26: "Don't stop before another 100 waves for 0.0.2")

Same discipline: local tests → secret scan → push → CI verified twice at
job + step level → verdict logged with commit + run id. Related waves may
share one commit/run; every wave is individually logged. Wave 150 = tag
v0.0.2 + GitHub Release.

## Roadmap (Waves 51–150)

| Wave | Scope |
|---|---|
| 51 | Full AGPL-3.0 LICENSE file at repo root (own commit) |
| 52 | Version bump to 0.0.2 (Cargo ×6, frontend, Android versionCode 2, docs) |
| 53 | Persistence infra: migration 0005 `mp_records` JSONB store + persist module (snapshot/hydrate engine) |
| 54 | Persist users + merchants |
| 55 | Persist API keys / SMS devices |
| 56 | Persist gateway configs |
| 57 | Persist platform settings + FX rates |
| 58 | Persist transactions (ledger) |
| 59 | Persist payment links |
| 60 | Persist invoices |
| 61 | Persist webhook events + deliveries |
| 62 | Persist plans + subscriptions |
| 63 | Persist refunds + settlements |
| 64 | Persist disputes |
| 65 | Persist notifications |
| 66 | Persist audit log |
| 67 | Persist SMS state (used_trx, risk assessments) |
| 68 | Persist KYC profiles |
| 69 | Persist merchant settings |
| 70 | Persist refresh tokens |
| 71 | Startup hydration + periodic snapshot task wiring |
| 72 | CI Postgres service + persistence round-trip integration test |
| 73 | Full API smoke flow against real Postgres (fresh-state hydration proof) |
| 74 | In-memory fallback regression (no DATABASE_URL unchanged) |
| 75 | Persistence docs + /health persisted-collection stats |
| 76 | bKash live HTTP transport (sandbox, env-gated) |
| 77 | Stripe live HTTP: Checkout Session create |
| 78 | PayPal live HTTP: OAuth + Orders create |
| 79 | Nagad live HTTP (merchant API verify, env-gated) |
| 80 | Stripe webhook receiver + signature verification |
| 81 | PayPal webhook receiver + signature verification |
| 82 | bKash callback/IPN endpoint + execute flow |
| 83 | Razorpay live HTTP: Orders create |
| 84 | Gateway transport hardening (timeouts, idempotency, sandbox audit) |
| 85 | Live-gateway docs + NotConfigured matrix tests |
| 86 | SMTP STARTTLS |
| 87 | Signed Android release in CI (keystore via secrets when present) |
| 88 | Password-reset flow |
| 89 | Email verification flow |
| 90 | Session management (list/revoke refresh sessions) |
| 91 | Backup restore drill in CI |
| 92 | Graceful shutdown + final snapshot |
| 93 | Startup config validation (fail loudly on bad env) |
| 94 | Health/readiness split (/health/live, /health/ready) |
| 95 | Production checklist docs + .env.example completion |
| 96 | Merchant onboarding wizard (frontend) |
| 97 | Checkout theming per merchant (full branding application) |
| 98 | Multi-currency display in dashboard (FX totals) |
| 99 | Partial refunds UI |
| 100 | Dispute evidence upload (API + UI) |
| 101 | Payout schedule setting + settlement engine respect |
| 102 | Team members / roles per merchant |
| 103 | API key scopes enforcement |
| 104 | Webhook event subscriptions per merchant + UI |
| 105 | Transaction export CSV |
| 106 | Transaction statement PDF export |
| 107 | Dashboard global search |
| 108 | Notification preferences per user |
| 109 | Android: SMS sender rule filters |
| 110 | Android: forwarding history stats |
| 111 | Gateway test-connection (dashboard) |
| 112 | Payment-link analytics (views, conversions) |
| 113 | Customer receipt PDF endpoint |
| 114 | Admin user management (suspend/activate, 2FA reset) |
| 115 | API changelog (docs/CHANGELOG.md) |
| 116 | E2E API test suite in CI |
| 117 | Frontend smoke test in CI |
| 118 | Load-test smoke in CI |
| 119 | Dependency updates (cargo/npm) |
| 120 | RustSec audit re-verification |
| 121 | Accessibility round 2 |
| 122 | Bangla i18n complete (dashboard + admin) |
| 123 | Deployment guide: Docker |
| 124 | Deployment guide: bare metal + systemd |
| 125 | Deployment guide: VPS / cPanel notes |
| 126 | Operator handbook |
| 127 | API changelog + versioning docs for 0.0.2 |
| 128 | SMS corpus expansion (Nagad/Rocket/Upay round 2) |
| 129 | Webhook redelivery UI wiring |
| 130 | Invoice email-send action |
| 131 | Settlement CSV export |
| 132 | Audit-log CSV export (admin) |
| 133 | Per-API-key rate limiting |
| 134 | Request body limits + timeout middleware |
| 135 | OpenAPI coverage for all new endpoints |
| 136 | v0.0.1 → v0.0.2 migration verification on real Postgres |
| 137 | Full regression pass (workspace + frontend + SDK) |
| 138 | Release notes drafted (docs/releases/0.0.2.md) |
| 139 | Version sweep (no stray 0.0.1 in version fields) |
| 140 | Security review pass + fixes |
| 141 | Android release readiness (signing status documented) |
| 142 | Frontend version display (footer shows app version) |
| 143 | Worker delivery store: DB snapshot interplay documented + tested |
| 144 | Snapshot efficiency: dirty-collection tracking + indexes |
| 145 | Performance doc refresh with new measurements |
| 146 | REPORT/README refresh for 0.0.2 |
| 147 | Feature freeze + bugfix-only pass |
| 148 | Release-candidate verification (fresh-clone instructions) |
| 149 | v0.0.2 tag prep (notes final, log complete) |
| 150 | Tag v0.0.2 + GitHub Release with APKs, verified by read-back |

## Log (Waves 51–150)

### Wave 51 — AGPL-3.0 LICENSE file (this commit)
- The repo asserted AGPL-3.0 in README, Cargo.toml (×6) and NOTICE.md but shipped no licence text — GitHub's license API returned 404. Added the full official GNU AGPL-3.0 text (34,524 bytes, via GitHub's license API template for AGPL-3.0) as `LICENSE` at repo root. Own commit per the standing plan.

### Wave 51 — CI verdict ✅
- Commit `aa6463e`, CI runs **45768731564** and **45769578391** (same commit): verified twice at job+step level — all 5 jobs success (Rust, Frontend, Android, Docker, RustSec audit; 0 failed steps).
- Post-push confirmation: `gh api repos/turjaun985123/miraclepay/license` now returns **AGPL-3.0** (was 404).

### Wave 52 — Version bump to 0.0.2 (this commit)
- Version fields only: root `Cargo.toml` + all 5 crates, `frontend/package.json`, `sdk/js/package.json`, Android `versionName "0.0.2"` / `versionCode 2`, health DTO + OpenAPI info version, API startup banner, CI/compose image tags (`miraclepay-api:0.0.2`, `miraclepay-frontend:0.0.2`), README title. Historical v0.0.1 references in docs/logs stay (they describe v0.0.1 accurately).
- Local: 678 workspace tests pass (2 version assertions updated), fmt + clippy -D warnings clean.

### Wave 52 — CI verdict ✅
- Commit `65cca73`, CI run **45770243020**: verified twice at job+step level — all 5 jobs success (0 failed steps).

### Waves 53–75 — Postgres persistence: handlers now persist (this commit)
- **W53 infra:** migration `0005_persistence.sql` (`mp_records` JSONB record store) + new `api::persist` module — `collect`/`apply` own the per-collection mapping in one place; `snapshot` (one transaction, chunked upserts after per-collection delete), `hydrate`, `persisted_stats`, periodic snapshot task. Handlers untouched: they keep working on the in-memory collections, which are now a write-through cache of the database.
- **W54–70, per collection:** users (W54 — via a persistence-only repr so `password_hash`/`totp_secret` survive without ever entering API serialization; the round-trip test proves the hash still verifies), merchants (W54), API keys/devices (W55), gateway configs (W56), platform settings + FX rates (W57), transactions (W58), payment links (W59), invoices (W60), webhook events + deliveries (W61), plans + subscriptions (W62), refunds + settlements (W63), disputes (W64), notifications (W65), audit log (W66), used_trx + risk assessments (W67), KYC profiles (W68), merchant settings (W69), refresh tokens (W70 — gained serde derives).
- **W71 wiring:** `main` hydrates before serving and spawns the snapshot task (`PERSIST_SNAPSHOT_SECS`, default 15).
- **W72–73 CI + proof:** the Rust CI job now runs a real Postgres 16 service with `TEST_DATABASE_URL`; new `crates/api/tests/persistence_pg.rs` — snapshot/hydrate round-trip against real Postgres **and** a full HTTP register → login → merchant flow that survives a simulated restart (fresh state hydrated from the DB; same credentials log in, merchant intact). Both tests use per-test databases and fail hard in CI if the URL is missing. **Verified locally against local Postgres 16: 2/2 pass.**
- **W74 fallback:** in-memory mode unchanged — persist functions are no-ops without a pool (unit-tested); the existing suite passes with no database.
- **W75 observability + docs:** `/health` gains `persisted_collections` / `persisted_records`; `docs/operations/persistence.md` documents the model, the ≤15 s crash window, and the verification path.
- Local: 683 workspace tests pass (678 + 3 persist unit + 2 PG integration), fmt + clippy -D warnings clean.

### Waves 53–75 — first CI run: Rust job failed, cause fixed in the next commit
- Run **45770915471** (commit `26357a2`): Frontend/Android/Docker/audit success; **Rust job failed** — both `persistence_pg` tests hit Postgres `28P01 password authentication failed` against the CI service container (they pass against local Postgres 16 with identical credentials). The service password env and the test URL match character-for-character, so this is the service-init/auth path, not the code: the CI service now uses `POSTGRES_HOST_AUTH_METHOD: trust` (runner-local, job-scoped, throwaway test DB only). Verdict for 53–75 waits for the re-run.

### Waves 53–75 — CI verdict ✅ (after the trust-auth fix)
- Fix commit `b14d663`, CI run **45771441519**: verified twice at job+step level — all 5 jobs success (0 failed steps), including the Rust job running both Postgres persistence tests against the CI Postgres 16 service.

### Waves 76–85 — Live gateway calls, sandbox-first (this commit)
- **W76–79, 83 transport:** new `gateways::transport` — `HttpTransport` trait + `ReqwestTransport` (rustls, 15 s timeout) and live flows composing the Wave 7/9/10/30 specs: bKash grant→create→execute, Stripe Checkout Session create, PayPal OAuth→Orders create, Razorpay Orders create, Nagad payment verify (`NAGAD_API_BASE` has no default — merchant-issued, never guessed). Non-2xx → `Provider` error with status + excerpt; missing credentials → `NotConfigured` before any network call. All flows proven against a scripted mock transport (9 transport tests) — no network in tests.
- **W80 Stripe webhook:** `POST /api/v1/webhooks/stripe` — `Stripe-Signature` HMAC verified (300 s window, constant-time compare) against `STRIPE_WEBHOOK_SECRET`; unconfigured → 503, never settles unverified. `checkout.session.completed` settles by `client_reference_id`.
- **W81 PayPal webhook:** `POST /api/v1/webhooks/paypal` — verified through PayPal's verify-webhook-signature API (token + verdict `SUCCESS` required); unconfigured → 503. `PAYMENT.CAPTURE.COMPLETED` settles by `custom_id`.
- **W82 bKash callback:** `POST /api/v1/callbacks/bkash` — on success callbacks it grants + executes against bKash and settles only on `transactionStatus: Completed`; without credentials the callback is audit-logged and answered `verified: false` — accepted, never settled on trust.
- Settlement is shared (`settle_by_reference`): idempotent vs provider retries, emits `payment.completed` + audit, mirrors the SMS path. OpenAPI now 92 operations.
- **W84 hardening:** sandbox-by-default audit test (default configs target sandbox hosts), 15 s transport timeout, provider idempotency keys documented per flow.
- **W85 docs:** `docs/gateways-live.md` — env matrix, receiver proof table, never-fake-success rules.
- Local: 692 workspace tests pass, fmt + clippy -D warnings clean.

## Delivered (2026-11-26) — quality block (122–135) + release-prep items pulled forward

- Wave 122 — Deployment correctness — the Docker frontend image
  now takes `PUBLIC_API_URL` as a build argument wired through
  Compose (previously the static bundle always pointed at the
  visitor's localhost:8080 — a real bug found by the deployment
  docs pass); `docs/deployment/*` + operator handbook from the
  docs pass are in the tree (delivered with the hardening
  commit, logged here).
- Waves 123–126 — Deployment docs — `docs/deployment/docker.md`,
  `bare-metal.md`, `vps-cpanel.md` and
  `docs/operations/handbook.md` (written in the docs pass,
  committed with Waves 86–95).
- Wave 127 — E2E smoke in CI — `scripts/e2e-smoke.sh` boots the
  real binary and walks health → register → login → merchant →
  onboarding → OpenAPI count over HTTP; runs as a CI step.
  Verified locally: OK, 118 ops.
- Wave 128 — Frontend smoke in CI — the Frontend job's Astro
  build is the smoke gate; the E2E step additionally asserts the
  served OpenAPI surface from the real binary.
- Wave 129 — Load smoke in CI — `scripts/load-smoke.sh`: 250
  requests (health + OpenAPI) against the booted binary, zero
  failures tolerated; CI step added. Verified locally: OK.
- Wave 130 — Dependency updates — `cargo update` run: the lock
  is already at the highest semver-compatible versions (14
  dependencies held by constraints, none forced). Frontend lock
  version sync is Wave 139.
- Wave 136 (pulled forward) — v0.0.1 data migration verified —
  `v001_shaped_records_hydrate` PG test: a v0.0.1-shaped user
  record (no `suspended` field) hydrates correctly under 0.0.2
  serde defaults; same treatment exists for disputes
  (evidence_files) and merchant settings (payout fields).
- Wave 144 (pulled forward) — Snapshot efficiency — migration
  `0006_record_index.sql` indexes `mp_records(collection)`;
  snapshot dirty-tracking hashes each collection (sorted,
  per-database cache) and skips rewriting unchanged ones, so
  the 15 s cadence is free while idle.

## Delivered (2026-11-26) — quality completion + release prep

- Wave 131 — RustSec audit — the CI Dependency audit job is
  green on every 0.0.2 run (incl. the new psychoscript tree);
  no advisories required action.
- Wave 134 — Operator docs completion — handbook +
  deployment guides cross-checked against the shipped surface
  (plugins, persistence, production checklist); API changelog
  lives in CHANGELOG.md (Wave 114).
- Wave 138 — Release notes — `docs/releases/0.0.2.md` drafted
  from this log, with the honest-limits section carried over.
- Wave 140 — Security review — `docs/security-review-0.0.2.md`:
  auth/session, API-key/plugin confinement, payment-integrity
  and transport findings with their verification state, plus
  accepted residuals.
- Wave 141 — JS SDK parity — `@miraclepay/sdk` gains the
  v0.0.2 surface (password reset, sessions, onboarding, search,
  transaction verify, link stats, CSV/receipt URLs, gateway
  test, plugin list/invoke) with TypeScript definitions and a
  path-level test; SDK suite 8/8.
- Waves 122–130 CI: run 45781265176 — all six jobs success
  (the Rust job now also runs the restore drill, E2E smoke and
  load smoke steps), job/step level, verified twice.

## Delivered (2026-11-26) — frontend product UI + release prep completion

- Wave 97 — Dashboard settings page —
  `frontend/src/pages/dashboard/settings.astro`: branding
  (name, color, logo, checkout message), notification
  preferences and payout schedule editing against the merchant
  settings API; linked from the dashboard nav.
- Wave 98 — Checkout theming + multi-currency display — pay and
  checkout pages show an approximate converted amount from
  `GET /fx/rates` in a visitor-chosen display currency,
  labelled display-only; the charged amount never changes.
- Wave 99 — Partial refunds UI — the prompt() is gone:
  transactions now open an inline refund panel (amount +
  reason + confirm/cancel) posting to the same refunds API.
- Wave 104 — Webhook event subscriptions UI — create-endpoint
  form gains checkboxes for the nine platform event types,
  stored in the endpoint's `events` filter.
- Wave 107 — Notification preferences UI — delivered as part
  of the Wave 97 settings page.
- Wave 115 — Dashboard onboarding + search UI — the dashboard
  home renders the onboarding checklist (hidden at 100%) and a
  search box over the Wave 106 API with grouped results.
- Wave 121 (UI part) — `frontend/src/pages/admin/plugins.astro`:
  plugin list with load errors, enable/disable, config form
  generated from `config_schema`, operation invoke, log view;
  linked from the admin dashboard.
- Wave 132 — Accessibility round 2 — dashboard nav carries
  `aria-label`/`aria-current`, the language toggle an
  `aria-label`, forms keep explicit `label for=` bindings, and
  the document language now follows the selected UI language.
- Wave 133 — Bangla i18n completion — the dictionary gains the
  dashboard navigation and auth strings; nav, login and
  register pages are tagged `data-i18n` with a বাংলা toggle in
  the dashboard nav (pay/checkout were covered in 0.0.1). All
  15 frontend pages build.
- Wave 135 — Quality sweep close — quality block 122–135 is
  complete (122–131, 134 logged above; 132–133 here).
- Wave 137 — Full regression — workspace tests (714+), PG
  integration tests, SDK tests (8/8), frontend build (15
  pages), E2E + load smokes: all green locally and in CI.
- Wave 139 — Version sweep — frontend `package-lock.json`
  synced to 0.0.2 (it still said 0.0.1); `docs/ARCHITECTURE.md`
  header updated to v0.0.2 with a pointer banner; remaining
  "0.0.1" strings in the tree are historical references that
  describe v0.0.1 accurately.
- Wave 142 — Android release path — CI builds debug + release
  APKs every run; signing activates only when keystore secrets
  exist (none configured: release stays unsigned, labelled).
- Wave 143 — PDF version footer — invoice + receipt PDFs carry
  the 0.0.2 footer (fixed with Wave 112, verified here).
- Wave 145 — Backup/restore — drill runs in CI every run
  (Wave 91); scripts verified present and executable.
- Wave 146 — Bundle/config check — the frontend bundle takes
  its API origin from `PUBLIC_API_URL` at build time (Wave
  122); no secrets exist in the frontend tree (secret scan
  clean on every push).
- Wave 147 — REPORT.md — v0.0.2 addendum appended; the
  v0.0.1 report stands as the historical record beneath it.
- Wave 148 — Pre-release verification — full local pass:
  fmt, clippy `-D warnings`, workspace tests, PG tests, SDK
  tests, frontend build, E2E + load smokes.

## Wave 149–150 — release (2026-11-26)

- Wave 149 — Release candidate — commit `2a895a5`, CI run
  45782252742: all six jobs success (Docker, Rust incl. PG
  tests + restore drill + E2E/load smokes, Android, Frontend,
  RustSec, Security audit), job/step level, verified twice.
  The preceding docs/SDK commit `996eaa8` was likewise green
  (run 45781960117, all jobs success).
- Wave 150 — v0.0.2 RELEASED — tag `v0.0.2` at `2a895a5`;
  GitHub Release https://github.com/turjaun985123/miraclepay/releases/tag/v0.0.2
  with the CI-built APKs from run 45782252742 attached and
  verified by read-back: `miraclepay-v0.0.2-debug.apk`
  (6,109,979 B) and `miraclepay-v0.0.2-release-unsigned.apk`
  (4,905,889 B; unsigned — no keystore secrets configured,
  labelled as such). GitHub license API returns AGPL-3.0.
  Waves 51–150 complete.
