# MiraclePay — Architecture

> Updated for v0.0.2 (Wave 139): the platform now persists to
> Postgres at runtime (§6 describes the v0.0.1 starting point;
> see `docs/operations/persistence.md` for the shipped design),
> gateways make live sandbox calls (`docs/gateways-live.md`),
> and a plugin system exists (`docs/plugins.md`). Sections that
> say "in v0.0.1" are historical and remain accurate for that
> release.

**Version:** v0.0.2
**Status:** Original, clean-room implementation. This document describes
the code as it exists in this repository — the Rust workspace, the
Postgres migrations, the Astro frontend, and the Android app — and
nothing else.

MiraclePay is a self-hosted payment-automation platform. A merchant
creates a payment, the payer is shown how to pay, the payer sends
money from their own wallet, and MiraclePay confirms the payment —
for manual / mobile-wallet gateways, by matching the confirmation
SMS that arrives on the merchant's phone — then notifies the
merchant's own systems by signed webhook and records the amount
toward the merchant's balance and eventual settlement.

---

## 1. System at a glance

```text
┌──────────────┐      HTTPS / JSON       ┌──────────────────────┐
│  Frontend    │ ──────────────────────► │  API                 │
│  Astro 7     │   /api/v1/*, Bearer JWT │  crates/api (Axum)   │
│  (static)    │ ◄────────────────────── │                      │
└──────────────┘                         │  ┌────────────────┐  │
       ▲                                 │  │ domain         │  │
       │ payer opens checkout /          │  │ gateways       │  │
       │ payment link                    │  │ sms_verify     │  │
┌──────────────┐                         │  └────────────────┘  │
│  Payer       │                         └───────┬──────────────┘
└──────┬───────┘                                 │
       │ sends money (wallet app / USSD)         │ signed webhook
       ▼                                         ▼
┌──────────────┐   POST SMS    ┌──────────────────────┐   ┌─────────────┐
│  Merchant's  │ ────────────► │  sms_verify intake   │   │  Merchant's │
│  Android     │  x-device-key │  (inside crates/api) │   │  webhook    │
│  phone + app │               └──────────────────────┘   │  endpoint   │
└──────────────┘                                          └──────▲──────┘
                                                                 │
┌──────────────┐   due deliveries, settlements, notifications    │
│  Worker      │ ────────────────────────────────────────────────┘
│  crates/worker│
└──────────────┘

Data shapes: crates/domain  •  Persistence target: Postgres (migrations/)
Runtime state in v0.0.1: in-memory AppState (see §6)
```

There are three deployable pieces and two supporting pieces:

| Piece | Location | Form |
|---|---|---|
| Backend API | `crates/api` (binary + library) | Axum HTTP service, default API base `http://localhost:8080` as assumed by the frontend |
| Background worker | `crates/worker` (binary + library) | Separate process; job engines in the library, a 60-second heartbeat loop in the binary in v0.0.1 |
| Frontend | `frontend/` | Astro 7, static output, dev port 4321 |
| Android forwarder | `android/` | Java app, package `com.miraclepay.app` |
| Database schema | `migrations/` | Postgres DDL, migrations `0001`–`0004` (the migration path — see §6) |

Everything else is library code shared by those pieces.

---

## 2. Workspace crates

The Rust workspace (`Cargo.toml`, resolver 2, edition 2021,
version `0.0.1`, AGPL-3.0) has five members. Dependencies point
inward: `api` and `worker` depend on `domain`; `sms_verify` depends
on `domain` and `gateways`; `gateways` depends on `domain`;
`domain` depends on no other MiraclePay crate.

### 2.1 `crates/domain` — `miraclepay-domain`

Framework-free domain types and rules. No HTTP, no database, no
network. One module per concept, each owning its own validation and
state transitions:

| Module | Responsibility |
|---|---|
| `money` | `Money` (integer minor units — poisha / paise / cents — never floating point) and `Currency` (`BDT`, `INR`, `PKR`, `USD`, `EUR`, 100 minor units each). Cross-currency arithmetic is an error, never an implicit conversion. |
| `transaction` | `Transaction` and the `TxStatus` state machine: `Pending → Processing → Completed / Failed`, `Pending → Failed / Expired`, `Completed → Refunded / PartiallyRefunded`. Final states cannot transition further. |
| `merchant`, `user` | Merchant record and user accounts / roles (`Admin`, `Merchant`, `Staff`, `Customer`), including registration-input validation. `password_hash` is never serialised. |
| `api_key` | API-key generation and checking. Only the hex SHA-256 of a raw key (`hash_key`) is stored; the raw key is shown once at creation. Keys carry scopes and a revoked flag. |
| `webhook` | `WebhookEvent`, `WebhookDelivery`, HMAC-SHA256 signing / verification (`sign_payload` / `verify_signature`), and the single source of retry policy: `max_attempts` (8) and exponential `backoff_seconds` (base 60 s, capped at 86 400 s). |
| `invoice`, `payment_link` | Invoices and reusable payment links, with their own lifecycles. |
| `refund`, `settlement` | Refunds, and settlements: a period's gross, the platform fee (`compute_fee`, basis points), and the net, which is always derived as `gross − fee` and never stored separately. |
| `subscription` | Plans and subscriptions. |
| `notification`, `audit`, `settings`, `stats` | Notifications, append-only audit entries (which must never contain secrets), platform settings, and statistics — including `compute_balance`, the pure function behind the merchant-balance endpoint (completed captures minus succeeded refunds). |
| `ids`, `errors` | Prefixed identifier generation and the shared `DomainError` type. |

### 2.2 `crates/gateways` — `miraclepay-gateways`

One adapter module per gateway, all behind one trait
(`traits.rs`), with shared request / response vocabulary in
`types.rs` and runtime selection in `registry.rs`
(`GatewayRegistry::from_configs` for a merchant's stored
configurations, `GatewayRegistry::all_unconfigured` otherwise).

Gateways present in v0.0.1, by `GatewayId`:

- **Bangladesh:** `bkash` (bKash), `nagad` (Nagad), `rocket`
  (Rocket), `upay` (Upay), `surecash` (SureCash)
- **India:** `upi` (UPI), `paytm` (Paytm), `phonepe` (PhonePe),
  `razorpay` (Razorpay)
- **Pakistan:** `easypaisa` (Easypaisa), `jazzcash` (JazzCash)
- **Global:** `stripe` (Stripe), `paypal` (PayPal)
- **Fallback:** `manual` (Manual)

Two adapter families share the contract:

- *Manual / SMS-verified gateways* — `initiate` returns
  send-money instructions built by `manual_instructions` (gateway
  name, merchant number, exact amount, and the reference the payer
  must copy exactly); confirmation arrives later, outside the
  trait, through the SMS pipeline (§4).
- *Hosted / API gateways* — `initiate` returns a redirect /
  provider reference (in v0.0.1 the Stripe, PayPal, and Razorpay
  adapters return a placeholder checkout URL and never claim a
  payment succeeded); `verify` asks the provider about a reference.

An unconfigured adapter is still listed, so a merchant can finish
setting it up, but it must not be offered for payment and its
`initiate` fails rather than starting a payment that can never
complete.

### 2.3 `crates/sms_verify` — `miraclepay-sms-verify`

The SMS verification engine: parse a forwarded payment SMS, match
it to a pending transaction, and produce a verdict. It performs no
I/O and mutates nothing; the caller persists the outcome.

| Module | Responsibility |
|---|---|
| `types` | `IncomingSms` — the raw wire format the Android app posts (`device_id`, `sender`, `body`, `received_at`, optional `sim_slot`) — and `ParsedSms`, the extracted fields (gateway, `amount_minor`, currency, sender / receiver numbers, `trx_id`, reference, balance, plus the raw sender and body for audit). A message is a payment candidate only when it has both an amount and a transaction id (`is_payment`). |
| `parser` | Generic field extractors: amount, balance, phone number, transaction id. |
| `patterns_bd` | Gateway detection for Bangladeshi senders / bodies. |
| `patterns_in_pk` | Gateway detection for Indian and Pakistani senders / bodies. |
| `matcher` | Scores each open candidate transaction against a `ParsedSms` and returns `Exact`, `Probable`, `NoMatch`, or `Duplicate` (see §4.3). |
| `verifier` | The two operations the rest of the platform calls: `parse_incoming` (raw SMS → `ParsedSms`) and `verify_sms` (parse, then match → `VerificationOutcome`). Currency is derived from the detected gateway's country (`BD → BDT`, `IN → INR`, `PK → PKR`) and is never guessed when no gateway is detected. |

### 2.4 `crates/api` — `miraclepay-api`

The Axum HTTP layer: thin handlers over `domain`, `gateways`, and
`sms_verify`. `routes.rs` only wires paths to handlers (51 method
handlers across 42 paths in v0.0.1); request / response shapes live
in `dto.rs`, cross-cutting input checks in `validation.rs` and
`pagination.rs`, authentication in `auth_middleware.rs` (JWT
session tokens for users, hashed API keys for programmatic and
device callers), and shared state in `state.rs` (§6).

Handler modules, one per resource: `auth`, `merchants`, `payments`
(payments and checkout), `payment_links`, `invoices`,
`transactions` (transactions and refunds), `gateways`, `api_keys`,
`sms`, `webhooks`, and `admin` (platform stats, users, merchants,
audit log, settings).

Endpoint groups, all under `/api/v1` unless noted:

- `GET /health` — liveness probe (no prefix).
- Auth: `POST /auth/register`, `POST /auth/login`, `GET /auth/me`.
- Merchants: `POST|GET /merchants`, `GET|PATCH /merchants/:id`,
  `GET /merchants/:id/balance`.
- Payments and checkout: `POST|GET /payments`, `GET /payments/:id`,
  `GET /checkout/:id` and `POST /checkout/:id/submit` (both public —
  the hosted checkout page needs no login; submit records the
  sender number and provider transaction id and moves a `Pending`
  transaction to `Processing`).
- Payment links: `POST|GET /payment-links`, `GET /payment-links/:id`,
  `POST /payment-links/:id/deactivate`, plus the public
  `GET|POST /pay/:slug` pay page.
- Invoices: `POST|GET /invoices`, `GET /invoices/:id`,
  `POST /invoices/:id/send`, `POST /invoices/:id/pay`,
  `POST /invoices/:id/cancel`.
- Transactions and refunds: `GET /transactions`,
  `GET /transactions/:id`, `POST /transactions/:id/refunds`,
  `GET /refunds`.
- Gateways: `GET /gateways`, `GET|PUT /merchants/:id/gateways`,
  `DELETE /merchants/:id/gateways/:gateway`,
  `GET /merchants/:id/gateways/status`.
- API keys: `POST|GET /api-keys`, `POST /api-keys/:id/revoke`.
- SMS intake (device-authenticated, §5): `POST /sms`,
  `POST /sms/parse` (dry-run parse preview; matches and mutates
  nothing).
- Webhooks: `GET /webhooks/events`, `GET /webhooks/deliveries`,
  `POST /webhooks/events/:id/redeliver`,
  `POST /webhooks/test-signature`.
- Admin: `GET /admin/stats`, `GET /admin/users`,
  `GET /admin/merchants`, `POST /admin/merchants/:id/status`,
  `GET /admin/audit`, `GET|PUT /admin/settings`.

There is deliberately no CORS layer in this crate; CORS is a
deployment concern for the fronting proxy / frontend host.

### 2.5 `crates/worker` — `miraclepay-worker`

Background jobs. The job engines are pure, storage-free library
functions; a caller loads due work, runs an engine, and persists
the result. In v0.0.1 the binary itself is a heartbeat loop (one
tick per 60 seconds) — production wiring that loads due work from
the store on each tick is the integration step, not part of the
engines.

| Module | Responsibility |
|---|---|
| `webhook_delivery` | One delivery attempt and the due-delivery run. Transport is injected through an `HttpTransport` trait, so the policy is testable with no network. Signs the body, sets the delivery headers (§5), and applies the domain retry policy: 2xx → `Delivered`; failure → `Failed` with `next_retry_at`, or `DeadLetter` once attempts are exhausted. |
| `settlement_job` | `collect_candidates` (completed, unsettled transactions in a half-open period `[start, end)`, grouped per merchant and currency), `build_settlements` (fee deducted via the domain `compute_fee`), `mark_paid`, `total_net`. Settlements are built for manual payout; the job never moves money itself. |
| `notification_job` | Notification dispatch over the domain notification records. |

---

## 3. The modular rule

The whole codebase is built to one rule, inherited as a design
principle for this platform: **every capability is an individually
callable, reusable unit, and no layer re-implements another
layer's job.**

- **Thin handlers.** An API handler validates ownership and input,
  delegates to a domain function, a gateway registry, or the SMS
  verifier, stores the result, and shapes the response. It contains
  no business arithmetic of its own — the merchant balance, for
  example, is the domain's `compute_balance`, not a sum written in
  the handler.
- **One operation = one endpoint.** Starting, confirming, and
  refunding a payment are separate calls; sending, paying, and
  cancelling an invoice are separate endpoints; redelivering a
  webhook is its own endpoint. Nothing is a hidden step inside
  another operation, so each operation can be retried, scheduled,
  audited, and authorised on its own.
- **One trait per gateway operation set.** Every gateway implements
  the single `PaymentGateway` trait — `id`, `display_name`,
  `is_configured`, `initiate`, `verify`, and `refund` (with a
  default `refund` that reports the operation as unsupported, which
  manual gateways inherit). Adding a gateway means writing one new
  adapter and registering it; checkout, transaction recording,
  webhooks, and settlement only ever see the trait. Adapters are
  used as `Box<dyn PaymentGateway>`, selected at runtime by
  `GatewayId`.
- **Single-responsibility domain functions.** Parsing, matching,
  scoring, fee computation, balance computation, signing, and
  backoff each live in exactly one place. The worker does not
  re-implement retry policy — it reads `max_attempts` and
  `backoff_seconds` from `domain::webhook` — and the SMS matcher
  does not re-implement money: amounts are compared as integer
  minor units from `domain::money`.
- **Same rule on the clients.** The frontend talks to the backend
  only through `frontend/src/lib/api.ts`; the Android receiver does
  no network work at all and hands each message to exactly one
  worker (§7).

---

## 4. Payment flow

The primary flow — a manual / SMS-verified mobile-wallet payment —
runs end to end as follows. Hosted gateways diverge only at steps
2–4: the payer is redirected to the provider instead of following
send-money instructions, and confirmation comes from the gateway's
`verify` rather than from an SMS.

### 4.1 Create payment

An authenticated merchant (or their server, using an API key) calls
`POST /api/v1/payments` with the merchant id, gateway id, amount in
minor units, currency, and optional customer email / phone and
caller reference. The handler checks that the caller owns the
merchant, resolves the gateway through that merchant's
`GatewayRegistry`, and calls the adapter's `initiate`. The
resulting `Transaction` is recorded in `Processing`, awaiting the
payer. Payments can also be started from a payment link
(`POST /pay/:slug`) or by paying an invoice
(`POST /invoices/:id/pay`).

### 4.2 Instructions / checkout

For a manual gateway, `initiate` returns the instructions built by
`manual_instructions`: open the wallet, choose Send Money, enter
the merchant's number, enter the exact amount, and enter the
payment's reference code exactly in the transfer's reference /
note field. The hosted checkout page (`GET /api/v1/checkout/:id`,
rendered by the frontend at `/checkout/[id]`) shows those
instructions. That reference — together with the exact amount —
is what later lets an observed transfer be matched back to this
payment, so the instructions stress copying it exactly and keeping
the confirmation message.

### 4.3 Payer sends money; SMS verification

1. The payer sends the money from their own wallet to the
   merchant's number, outside MiraclePay.
2. The wallet's confirmation SMS arrives on the merchant's Android
   phone. The forwarder app (§7) POSTs it, raw and uninterpreted,
   to the SMS intake endpoint with its device key.
3. The intake handler (`handlers/sms.rs`) authenticates the device
   (§5), scopes the request to that key's merchant — one merchant's
   device can never settle another merchant's payment — and runs
   `sms_verify::verify_sms` over that merchant's `Pending` /
   `Processing` transactions.
4. **Parsing.** `parse_incoming` detects the gateway (Bangladeshi
   patterns first, then India / Pakistan patterns) and extracts
   amount, provider transaction id, sender number, and balance.
   A blank body is rejected; a message with neither an amount nor
   a transaction id is not a payment SMS.
5. **Matching — the score model** (`matcher::score_candidate`).
   A provider transaction id that has already been consumed is a
   `Duplicate` before anything else is considered, so a replayed
   SMS can never settle a second transaction. Each remaining
   candidate is scored additively, one award per independent
   signal:

   | Signal | Points |
   |---|---|
   | Provider transaction id matches | 100 |
   | Amount matches (minor units) | 50 |
   | Sender number matches | 30 |
   | Receiver number matches, or SMS gateway matches the transaction's gateway | 20 |

   A missing amount never matches. The verdict:

   - **Score ≥ 80 → `Exact`** — a transaction-id match alone
     (100), or amount + sender (50 + 30). Settles automatically.
   - **Score ≥ 50, with amount and currency confirmed →
     `Probable`** — reported to the merchant for a further check;
     it does not settle on the automatic path.
   - **Anything else → `NoMatch`.**

   The intake response mirrors this: `verified` and `probable`
   return 200 with the transaction id; `duplicate`, `unmatched`,
   and `not_payment` settle nothing (`unmatched` / `not_payment`
   return 202).

### 4.4 Completion, webhook, balance, settlement

On an exact match — and only on an exact match — the intake
handler completes the transaction (`Processing → Completed` under
the domain state machine), records the provider transaction id,
and reserves it in the used-transaction-id set (replay
protection). It then:

- queues a `payment.completed` `WebhookEvent` for the merchant;
- makes the amount count toward the merchant's balance
  (`GET /merchants/:id/balance`, via `compute_balance`: completed
  captures minus succeeded refunds); and
- makes the transaction eligible for the worker's settlement job,
  which groups completed, unsettled transactions per merchant and
  currency over a period, deducts the platform fee, and produces a
  `Pending` settlement for manual payout.

The webhook is delivered by the worker (§2.5): the JSON event body
is signed, POSTed to the merchant's webhook URL, and retried with
exponential backoff until it succeeds or is dead-lettered. A
merchant can inspect events and deliveries and trigger a redelivery
from the API (`/webhooks/*`).

The checkout page can also move a payment forward by hand:
`POST /checkout/:id/submit` accepts the sender number and provider
transaction id from the payer and moves a `Pending` transaction to
`Processing`, after which the SMS match completes it as above.

---

## 5. Security notes

What v0.0.1 actually implements — including the parts that are
explicitly placeholders.

- **Signed webhooks.** Every delivery body is signed with
  HMAC-SHA256 keyed by the merchant's webhook secret
  (`domain::webhook::sign_payload`). The worker sends the signature
  as `x-miraclepay-signature`, alongside `x-miraclepay-event` and
  `x-miraclepay-delivery`; receivers verify with
  `verify_signature` (constant-time comparison via the `hmac`
  crate), and `POST /webhooks/test-signature` exists so a merchant
  can check their own verification code. The signing secret travels
  on the worker's delivery job, not on the persisted delivery
  record.
- **Hashed API keys.** Raw API keys are never stored. State and
  the `api_keys` table keep only `key_hash` (hex SHA-256) and a
  display prefix; the raw key is shown once at creation. Lookups
  hash the presented key and compare hashes. Keys are scoped and
  revocable.
- **Device authentication for SMS intake.** The Android app
  presents its raw API key in the `x-device-key` header. The
  server hashes it immediately — the raw key is never stored,
  logged, or echoed — and requires the key to exist, be unrevoked,
  and carry the payments-write scope (or full access). The key's
  merchant is the only merchant whose transactions the forwarded
  SMS may settle.
- **Masked gateway secrets.** Gateway configuration responses never
  return a stored API secret: `handlers/gateways.rs` replaces it
  with the constant mask `"***"` (`MASKED_SECRET`). Audit entries
  likewise must never contain passwords, full API keys, or card
  numbers.
- **Password hashes never leave the domain type.** `User` skips
  `password_hash` during serialisation, and the auth handlers build
  their response JSON field by field so the hash cannot leak.
- **Placeholder password hashing — production must replace it.**
  In v0.0.1, registration and login hash passwords with the
  API-key SHA-256 helper over `email::password`, and the code is
  commented, in both places, `placeholder — production must use
  argon2/bcrypt`. This is a fast, unsalted-per-user construction
  and is **not** suitable for production. The migration path is to
  swap that one helper for Argon2id (the stored-hash format the
  domain tests already use as their example) before any real
  deployment. No other part of the system depends on the
  placeholder: callers pass an already-computed hash into the
  state layer (see `AppState::seed_admin`).
- **Session tokens.** User authentication uses JWTs signed with a
  secret taken from the `JWT_SECRET` environment variable. When it
  is unset, the API falls back to a fixed, publicly known
  development secret (`dev-secret-change-me`) so a local instance
  starts without ceremony; that fallback must be overridden in
  every deployed environment.
- **Replay protection.** A provider transaction id is recorded in
  the used set the moment it settles a transaction and is checked
  before matching, so the same confirmation SMS cannot settle two
  payments.
- **No CORS in the API crate** (see §2.4): cross-origin policy is
  set by the deployment's proxy / frontend host, not by permissive
  defaults in the service.

---

## 6. Data model

### 6.1 Postgres migrations (the persistence target)

Four migrations under `migrations/` define the full Postgres
schema, in the same shapes as the domain types:

| Migration | Tables |
|---|---|
| `0001_core.sql` — core identity | `users`, `merchants`, `api_keys`, `gateway_configs`, `platform_settings` |
| `0002_payments.sql` — payments | `transactions`, `payment_links`, `invoices`, `checkout_sessions` |
| `0003_billing.sql` — billing | `plans`, `subscriptions`, `refunds`, `settlements` |
| `0004_ops.sql` — operations | `webhook_events`, `webhook_deliveries`, `notifications`, `audit_log`, `sms_messages`, `used_trx_ids` |

Notable correspondences with the code: `api_keys.key_hash` is
unique and the raw key appears nowhere; `merchants` carries
`webhook_url` and `webhook_secret`; `used_trx_ids` is the persisted
form of the replay-protection set; `webhook_deliveries` carries
`status` and `next_retry_at`, indexed for the worker's due-delivery
scan; `sms_messages` is indexed by merchant / match status and by
parsed transaction id.

### 6.2 v0.0.1 runtime: in-memory state

**Runtime state in v0.0.1 is in-memory; Postgres is wired but
not yet the store of record.** Since Wave 2 the API connects a
SQLx pool at startup when `DATABASE_URL` is set and runs the
migrations; `/health` reports `"db": "postgres"` or
`"in_memory"`. Handlers still read and write the in-memory
`AppState` collections described below — the pool is the
foundation the persistence waves build on, and the honest limit
of v0.0.1. `AppState`
(`crates/api/src/state.rs`) keeps every record in memory, behind
`Arc<RwLock<…>>`:

- keyed `HashMap<String, _>` collections for users, merchants,
  transactions, payment links, invoices, refunds, settlements,
  plans, subscriptions, API keys (keyed by key *hash*, never by the
  raw key), and per-merchant gateway configurations;
- append-only `Vec<_>` logs for webhook events, webhook
  deliveries, notifications, and the audit log;
- a `HashSet` of used provider transaction ids, and the platform
  settings value.

Handlers lock only the one collection they need and drop the guard
promptly. Consequences of the in-memory design, stated plainly:
state is lost on restart (apart from the admin account re-seeded
on first boot of an empty state), and the service is a single
process — there is no shared store for multiple API instances.

**Migration path:** the in-memory collections and the migrations
were designed with the same shapes on purpose — each `HashMap`
corresponds to a table, each append-only log to its ops table, and
the domain types serialise as the persisted records. Moving to
Postgres means replacing the `AppState` collections with
repository implementations over migrations `0001`–`0004` behind
the same handler-facing interface; handlers, domain functions,
gateway adapters, the SMS engine, and the worker engines do not
change, because none of them know where the records live today
beyond `AppState`.

---

## 7. Frontend

`frontend/` is an Astro 7 project (`astro ^7.4.13`) with a single
dependency, built with `output: 'static'`: plain HTML / CSS / JS
that any static host — or the Rust backend itself — can serve. No
SSR adapter is used in v0.0.1.

- **Talks only to the API.** The frontend has no data access of its
  own. Every page fetches from the Rust API through the one client,
  `src/lib/api.ts` (framework-free TypeScript over `fetch`), which
  reads the API base from `PUBLIC_API_URL` (default
  `http://localhost:8080`), attaches the stored session token as
  `Authorization: Bearer …` (`localStorage` key `mp_token`), and
  surfaces the server's `{ error }` message on failure.
- **Pages** (`src/pages/`): landing (`index`), `login`, `register`;
  the merchant dashboard — overview, `transactions`, `invoices`,
  `payment-links`, `gateways`, `api-keys`, `webhooks`; the public
  checkout page (`checkout/[id]`) and payment-link pay page
  (`pay/[slug]`); and an admin overview (`admin/index`).
- **Shared pieces:** `src/layouts/BaseLayout.astro` and
  `src/components/DashboardNav.astro`. Pages contain presentation
  and calls into `api.ts` only — no endpoint URL is constructed
  anywhere else, mirroring the backend's modular rule.

---

## 8. Android forwarder

`android/` is a plain Java Android app (no Kotlin), package
`com.miraclepay.app`, `versionName "0.0.1"`, compile / target SDK
34, min SDK 23, Java 17, depending on AppCompat, Material, and
WorkManager 2.9.1. Its design principle matches the server's:
**the app is a dumb pipe.** It forwards what the phone received,
exactly as received; all parsing, gateway detection, and matching
happen server-side in `sms_verify`.

Components (`app/src/main/java/com/miraclepay/app/`):

- **`SmsReceiver`** — a `BroadcastReceiver` for
  `Telephony.SMS_RECEIVED`, declared exported but guarded by the
  `BROADCAST_SMS` permission at priority 999. It does as little as
  possible on the broadcast thread — no network, database, or
  filesystem work: it reassembles multi-part messages (grouped by
  sender, stamped with the first part's time), checks that
  forwarding is enabled and configured, and enqueues one
  WorkManager request per message, tagged `sms_forward`, requiring
  a connected network, with exponential backoff from 30 seconds.
  Each SMS is its own unit of work — deliberately no unique-work
  name, so a new message can never replace a still-pending forward
  of an earlier one.
- **`SmsForwardWorker`** — the WorkManager `Worker` that performs
  the HTTP POST off the main thread. Result policy: success on a
  2xx response; permanent failure for an unconfigured app, a blank
  sender / body, or a 4xx rejection (retrying the identical request
  cannot help); `Result.retry()` for 5xx responses and network
  errors, letting WorkManager's backoff own the timing. The message
  body is never written to the log — payment SMS carry amounts,
  transaction ids, and account details.
- **`ApiClient`** — builds the `IncomingSms` JSON (`device_id`,
  `sender`, `body`, `received_at`, `sim_slot` when known) and POSTs
  it with the device key in the `x-device-key` header. As written
  in v0.0.1 it posts to the path constant `/api/v1/sms/incoming`;
  the API's routed intake path is `POST /api/v1/sms` — aligning
  that constant with the route is a known v0.0.1 integration item.
- **`ConfigStore`** — on-device configuration: server base URL,
  the device API key issued from the dashboard, a stable device id
  generated on first use, an optional merchant label, and the
  forwarding on/off switch. `isConfigured` requires a usable URL
  and a non-empty device key; `isEnabled` gates the receiver.
- **`MainActivity`** (plus the `SettingsActivity` declared in the
  manifest) — status and configuration UI.
- **`BootReceiver`** — listens for `BOOT_COMPLETED` /
  `QUICKBOOT_POWERON` so forwarding is re-armed after a reboot.

Permissions requested in the manifest: `RECEIVE_SMS`, `READ_SMS`,
`INTERNET`, `ACCESS_NETWORK_STATE`, `RECEIVE_BOOT_COMPLETED`,
`POST_NOTIFICATIONS`, `FOREGROUND_SERVICE`, and `WAKE_LOCK`.
Backups are disabled (`allowBackup="false"`), so the stored device
key is not carried into device backups.

End to end, the phone's only jobs are *receive, enqueue durably,
forward verbatim, retry transient failures* — WorkManager owns
surviving process death and offline periods, so a confirmation SMS
is not lost because the phone was offline when it arrived.

---

## 9. Repository layout

```text
crates/domain/       domain types and rules (framework-free)
crates/gateways/     one adapter per gateway behind PaymentGateway
crates/sms_verify/   SMS parsing, scoring, and matching
crates/api/          Axum service: routes, thin handlers, AppState
crates/worker/       webhook delivery, settlement, notification jobs
migrations/          Postgres DDL 0001–0004 (persistence target)
frontend/            Astro 7 static site; talks only to the API
android/             Java SMS-forwarder app (com.miraclepay.app)
docs/                this document and companion documentation
```
