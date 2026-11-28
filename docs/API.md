# MiraclePay API Reference

**Version:** 0.0.1
**Status:** Clean-room implementation of MiraclePay v0.0.1. This document describes the API exactly as wired in `crates/api/src/routes.rs` — 51 method handlers across **42 distinct paths** — with handler behaviour taken from `crates/api/src/handlers/`, `crates/domain/`, `crates/sms_verify/`, and `crates/worker/`.

---

## 1. Overview

| Item | Value |
|---|---|
| Default base URL | `http://localhost:8080` |
| Bind address / port | `0.0.0.0:$PORT`, where `PORT` defaults to `8080` (see `crates/api/src/main.rs`) |
| Versioned prefix | `/api/v1` for almost every endpoint |
| Unversioned paths | `GET /health` and `GET|POST /pay/:slug` only |
| Content type | `application/json` for request and response bodies |
| Path parameters | Written below as `{id}`, `{slug}`, `{gateway}`; in the Axum 0.7 router they appear as `:id`, `:slug`, `:gateway` |

Example:

```http
GET /api/v1/payments HTTP/1.1
Host: localhost:8080
Authorization: Bearer <token>
```

There is deliberately no CORS layer in the router; CORS is a deployment concern for the fronting proxy / frontend host.

### Conventions

- **Timestamps** are RFC 3339 / ISO 8601 UTC strings (for example `received_at` in the SMS payload).
- **IDs** are opaque strings with a type prefix (for example merchant, payment, transaction, invoice, API-key and webhook-event IDs minted by `crates/domain/src/ids.rs`). Treat them as opaque: copy them from responses, do not construct them.
- **Ownership scoping:** for non-admin callers, merchant-scoped endpoints only ever expose merchants the caller owns, and list filters such as `?merchant_id=` can narrow that scope but never widen it. Admins (JWT role `admin`) see all merchants.
- **List endpoints** accept endpoint-specific filters (for example `merchant_id`, `status`, `event_id`, `limit`) as query parameters; check the handler for the exact set on each route. Webhook event lists are newest first and capped at 100 entries.

---

## 2. Authentication

MiraclePay has three distinct credentials. They are **not interchangeable**.

### 2.1 JWT Bearer — dashboard / user endpoints

Used by most endpoints in this reference (labelled **JWT** in the tables).

```http
Authorization: Bearer <jwt>
```

- Obtain a token from `POST /api/v1/auth/register` or `POST /api/v1/auth/login`.
- Tokens are HS256 JSON Web Tokens: `base64url(header).base64url(payload).base64url(signature)`, where the signature is HMAC-SHA256 keyed by the server's JWT secret.
- Claims are `sub` (user ID), `role` (for example `admin`, `merchant`, `staff`, `customer`), an optional `merchant_id` scope, and `exp`.
- Tokens expire **24 hours** after they are minted. A missing, malformed, wrongly signed, or expired token is rejected with `401`.
- Admin endpoints additionally require `role == "admin"`; an authenticated non-admin caller gets `403`.

### 2.2 API keys — server-to-server credentials (`mp_live_…`)

- A raw API key is `mp_live_` followed by **32 lowercase hex characters** (`KEY_PREFIX` / `KEY_HEX_LEN` in `crates/domain/src/api_key.rs`).
- Keys are created with `POST /api/v1/api-keys` (JWT required). The **raw key is returned exactly once**, in the create response; only its SHA-256 hash and its first-12-character display prefix (`mp_live_…`, safe to show) are stored afterwards, so a lost raw key cannot be recovered — revoke it and create a new one.
- Present a server key as a bearer credential or in the dedicated key header:

  ```http
  Authorization: Bearer mp_live_<32 hex characters>
  ```
  ```http
  X-Api-Key: mp_live_<32 hex characters>
  ```

- Key scopes (snake_case in JSON): `payments_read`, `payments_write`, `invoices_write`, `webhooks_manage`, `refunds_write`, `full_access`. `full_access` implies every other scope; every other scope allows only itself (a write scope does **not** implicitly grant the matching read scope).
- Revoked keys must be rejected even if the key value otherwise matches.

> **v0.0.1 wiring note (read this before integrating):** in the router as shipped, the handlers other than the SMS endpoints take the `AuthUser` JWT extractor described in §2.1 — there is no separate API-key extractor wired into those routes. The one place a raw `mp_live_` key is actually read from a header in v0.0.1 code is the SMS ingest path, via `x-device-key` (§2.3). Plan integrations against the per-endpoint **Auth** column in §4, not against a blanket assumption that an API key unlocks every route.

### 2.3 Device key — SMS ingest (`x-device-key`)

Used only by `POST /api/v1/sms` and `POST /api/v1/sms/parse` (labelled **Device key**). There is no JWT on these endpoints.

```http
x-device-key: mp_live_<32 hex characters>
```

- The server hashes the presented raw key (`hash_key`, SHA-256 hex) and matches it against stored API keys. The raw key is never stored, logged, or echoed back.
- The key must exist, must not be revoked, and must carry `payments_write` (or `full_access`).
- The key's `merchant_id` is the only merchant the device can act for: an SMS forwarded by one merchant's device can never settle another merchant's payment.
- Any failure — missing/empty header, unknown key, revoked key, insufficient scope — is `401`.

### 2.4 Public endpoints

Labelled **Public** in the tables: `GET /health`, `POST /api/v1/auth/register`, `POST /api/v1/auth/login`, `GET /api/v1/gateways`, the checkout pair (`GET /api/v1/checkout/{id}`, `POST /api/v1/checkout/{id}/submit`), the public payment-link pair (`GET /api/v1/payment-links/public/{slug}`, `POST /api/v1/payment-links/public/{slug}/pay`), and invoice payment (`POST /api/v1/invoices/{id}/pay`). These take no `AuthUser` argument in their handlers; the resource ID / slug in the path is the capability.

---

## 3. Money and errors

### 3.1 Money format

- All amounts are **integers in the currency's minor unit** — never floats. Fields are named accordingly, for example `amount_minor`.
- A money object serialises as:

  ```json
  { "amount_minor": 150000, "currency": "BDT" }
  ```

  `150000` with `BDT` is 1,500.00 taka (display form: `BDT 1,500.00`).
- Supported currency codes (ISO 4217, from `Currency` in `crates/domain/src/money.rs`): `BDT`, `INR`, `PKR`, `USD`, `EUR`. Matching on input is case-insensitive and surrounding whitespace is ignored; unsupported codes are rejected.
- Every supported currency subdivides into **100 minor units per major unit** (poisha, paise, cents, …).
- Amounts in different currencies are never implicitly converted or combined; mixing currencies is an error (`400`).

### 3.2 Error format

Every error uses the Wave-45 envelope (full catalogue:
`docs/errors.md`):

```json
{ "error": "<human-readable message>", "code": "<stable_code>", "status": 400 }
```

`error` is for humans and may be reworded; `code` is the stable
machine-readable field clients should branch on.

| HTTP status | Meaning |
|---|---|
| `400` | Malformed request or validation failure, including invalid amounts and currency mismatches |
| `401` | Not authenticated — missing/invalid/expired JWT, or missing/invalid/insufficient device key (`"error": "unauthorized"`) |
| `403` | Authenticated but not allowed (for example a non-admin calling an admin endpoint) |
| `404` | The requested resource does not exist |
| `409` | The request conflicts with current state (for example acting on an already-settled resource) |
| `500` | Internal failure. The body is always `{"error": "internal error"}`; real detail is logged server-side only and never leaked. Gateway provider/network failures also surface this way |

---

## 4. Endpoints

> **Canonical catalogue:** the OpenAPI document served at
> `GET /openapi.json` (89 operations, generated from the route
> table with drift-guard tests) is the source of truth for the
> endpoint list. The tables below cover the core surface and the
> conventions; newer additions (FX, disputes, KYC, merchant
> settings/analytics, fraud flags, metrics) are documented in the
> OpenAPI document and the wave log (`WAVES.md`).

Auth key: **JWT** = §2.1 · **API key / JWT** = see §2.2 note · **Device key** = §2.3 · **Admin** = JWT with role `admin` · **Public** = §2.4.

### 4.1 Health

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/health` | Public | Liveness probe. Returns `{"status": "ok", "version": "0.0.1", "name": "MiraclePay"}`. |

### 4.2 Auth

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/auth/register` | Public | Register a user account. Body: `{name, email, password}`. |
| POST | `/api/v1/auth/login` | Public | Log in. Body: `{email, password}`; returns a JWT (24 h, see §2.1). |
| GET | `/api/v1/auth/me` | JWT | Return the authenticated caller's own profile. |

### 4.3 Merchants

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/merchants` | JWT | Create a merchant owned by the caller. |
| GET | `/api/v1/merchants` | JWT | List the caller's own merchants. |
| GET | `/api/v1/merchants/{id}` | JWT | Get one merchant (owner or admin). |
| PATCH | `/api/v1/merchants/{id}` | JWT | Update a merchant; all fields optional: `{business_name, phone, webhook_url}`. |
| GET | `/api/v1/merchants/{id}/balance` | JWT | Get the merchant's balance. |

### 4.4 Gateways

Gateway IDs are lowercase strings: `bkash`, `nagad`, `rocket`, `upay`, `surecash` (BD); `upi`, `paytm`, `phonepe`, `razorpay` (IN); `easypaisa`, `jazzcash` (PK); `stripe`, `paypal`, `manual` (GLOBAL) — 14 gateways in total.

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/api/v1/gateways` | Public | List all gateways with ID, display name, and primary country / `GLOBAL`. |
| GET | `/api/v1/merchants/{id}/gateways` | JWT | List the merchant's gateway configurations (secrets are masked in the view). |
| PUT | `/api/v1/merchants/{id}/gateways` | JWT | Create or replace (upsert) one gateway configuration for the merchant; the body is a gateway config naming the gateway. |
| DELETE | `/api/v1/merchants/{id}/gateways/{gateway}` | JWT | Delete the merchant's configuration for that gateway. |
| GET | `/api/v1/merchants/{id}/gateways/status` | JWT | Configured vs. unconfigured gateway IDs for the merchant, in canonical gateway order. |

### 4.5 Payments & Checkout

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/payments` | JWT | Create a payment for a merchant the caller owns (admins: any merchant). Body includes `merchant_id` and a `gateway` ID such as `bkash` or `stripe`. Queues a `payment.created` webhook event. |
| GET | `/api/v1/payments` | JWT | List payments; filters include `merchant_id` and `status` (for example `pending`, `completed`). |
| GET | `/api/v1/payments/{id}` | JWT | Get one payment. |
| GET | `/api/v1/checkout/{id}` | Public | Checkout details for a payment, for rendering the checkout page. Includes `receiving_number` (`string \| null`, payer-safe — see §4.6.1): the merchant's receiving wallet number for the transaction's gateway, or `null` when none is configured for that gateway. |
| POST | `/api/v1/checkout/{id}/submit` | Public | Submit a checkout payment. Body: `{sender_number, provider_trx_id}` — the number the customer sent from and the provider transaction ID from their confirmation. |

### 4.6 Payment Links

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/payment-links` | JWT | Create a payment link for a merchant the caller owns. Body includes `merchant_id` and `title`; the title is slugified into the public slug. The amount may be fixed or open. |
| GET | `/api/v1/payment-links` | JWT | List payment links; optional `merchant_id` filter. |
| GET | `/api/v1/payment-links/{id}` | JWT | Get one payment link. |
| POST | `/api/v1/payment-links/{id}/deactivate` | JWT | Deactivate a payment link so its public page no longer accepts payment. |
| GET | `/api/v1/payment-links/public/{slug}` | Public | Public pay page data for a link: title, description, fixed amount (or `null` for open-amount), the merchant's trading name, branding, the gateway ids offered, and `receiving_numbers` (payer-safe — see §4.6.1). Payer-safe fields only — no merchant id, credentials, or config. (The browser page itself is `/pay/{slug}`, served by the embedded frontend.) |
| POST | `/api/v1/payment-links/public/{slug}/pay` | Public | Pay a link. Body: `gateway` (required), `amount_minor` (required for open-amount links, ignored when the link has a fixed amount), optional `customer_phone` / `customer_email`. Returns `{transaction_id, status}` with `status` starting as `pending`. |

#### 4.6.1 Receiving numbers on public responses (v0.0.4)

So a payer knows where to send the money, the two public read endpoints above expose the merchant's **receiving wallet number** — the number configured as `merchant_number` on the merchant's gateway config (`GatewayConfig.merchant_number`). This is payer-safe by design: it is the number the merchant publicly asks payers to send money to.

| Field | Endpoint | Type | Semantics |
|---|---|---|---|
| `receiving_number` | `GET /api/v1/checkout/{id}` | `string \| null` | The receiving number for **the transaction's (selected) gateway**. `null` when the merchant has not configured an exposable receiving number for that gateway. |
| `receiving_numbers` | `GET /api/v1/payment-links/public/{slug}` **and** `GET /api/v1/checkout/{id}` | array of `{gateway, number}` | One entry per **offered** gateway that has an exposable receiving number (on checkout this list powers the gateway picker), for example `{"gateway": "bkash", "number": "01…"}`. Offered gateways **without** a configured number are omitted entirely (never listed with a `null` number); the array is empty when no offered gateway has one. |

> **Exposure rule (all surfaces):** a number is exposable only when the gateway is a number-based manual gateway (bkash, nagad, rocket, upay, surecash, upi, paytm, phonepe, easypaisa, jazzcash, manual), its config is **enabled**, the stored number is non-blank, and it passes the gateway adapter's own identifier validation. Credential gateways (stripe, paypal, razorpay) never expose a number, even if one is stored. These are published pay-to addresses, so listing every offered gateway's number is payer-safe.

> **Security note:** only the receiving number is public. Gateway credentials — `api_key`, `api_secret`, and the rest of the gateway config (`extra`) — are **never** part of any public response, including the two endpoints above. The masked gateway views returned to authenticated merchant endpoints are unchanged.

### 4.7 Invoices

Invoice statuses: `draft`, `sent`, `paid`, `overdue`, `cancelled`, `void`.

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/invoices` | JWT | Create an invoice. Body includes `merchant_id`, `customer_name`, `customer_email`, and line items. |
| GET | `/api/v1/invoices` | JWT | List invoices; optional `merchant_id` and `status` filters. Responses include `subtotal_minor`, `tax_minor`, and the grand total derived from the line items. |
| GET | `/api/v1/invoices/{id}` | JWT | Get one invoice. |
| POST | `/api/v1/invoices/{id}/send` | JWT | Mark an invoice as sent to the customer. |
| POST | `/api/v1/invoices/{id}/pay` | Public | Pay an invoice. Body: `{gateway, sender_number?}` — for example `gateway: "bkash"`. Returns `{transaction_id, invoice_id}`. |
| POST | `/api/v1/invoices/{id}/cancel` | JWT | Cancel an invoice (rejected with `409` if it is already paid). |

### 4.8 Transactions & Refunds

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/api/v1/transactions` | JWT | List transactions; optional `merchant_id` and status filters, owner-scoped for non-admins. |
| GET | `/api/v1/transactions/{id}` | JWT | Get one transaction. |
| POST | `/api/v1/transactions/{id}/refunds` | JWT | Create a refund against a transaction. Body: optional `amount_minor` (omit for the full remaining refundable amount) and optional `reason`. Queues a `payment.refunded` webhook event. |
| GET | `/api/v1/refunds` | JWT | List refunds; optional `merchant_id` filter, owner-scoped for non-admins. |

### 4.9 Webhooks

Management endpoints only — actual delivery is performed by the background worker (see §5).

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/api/v1/webhooks/events` | JWT | List webhook events for merchants the caller owns (admins: all). Newest first, capped at 100; optional `merchant_id` filter. |
| GET | `/api/v1/webhooks/deliveries` | JWT | List webhook deliveries visible to the caller; optional `event_id` filter. |
| POST | `/api/v1/webhooks/events/{id}/redeliver` | JWT | Queue a fresh delivery attempt for an existing event (the merchant must have a webhook URL configured). Returns `202`; this enqueues, it does not itself POST to the merchant. |
| POST | `/api/v1/webhooks/test-signature` | JWT | Check a signature without delivering anything. Body: `{secret, payload, signature}`; returns `{"valid": true|false}` — `false` also for malformed signatures. |

### 4.10 SMS

Both endpoints take the `IncomingSms` JSON body shown in §6 and are authenticated with the device key (§2.3).

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/sms` | Device key | Ingest one forwarded SMS and verify it against the key's merchant's pending/processing transactions. See the outcome table in §6. Only a `verified` outcome mutates state: the transaction is completed, its provider transaction ID is reserved against reuse, and a `payment.completed` webhook event is queued. |
| POST | `/api/v1/sms/parse` | Device key | Dry run: return the structured parse of one SMS without matching or mutating anything. A message that does not parse is **not** an API error here — the body is `{"error": "<parser message>"}` so a device can debug what the server extracted. |

### 4.11 API Keys

| Method | Path | Auth | Description |
|---|---|---|---|
| POST | `/api/v1/api-keys` | JWT | Create a key. Body: `{merchant_id, name, scopes}` — the merchant must be owned by the caller (or the caller is an admin). The response is the only place the raw `mp_live_…` key ever appears; it also carries the key `id`, `name`, and public prefix. |
| GET | `/api/v1/api-keys` | JWT | List keys for a merchant (`?merchant_id=` required). Views never include the raw key or its hash — only `id`, `name`, `key_prefix`, `scopes`, and revocation/usage metadata. |
| POST | `/api/v1/api-keys/{id}/revoke` | JWT | Revoke a key. Revoked keys are rejected everywhere, including SMS ingest. Note: this is a `POST`, not a `DELETE`, in the v0.0.1 router. |

### 4.12 Admin

Every endpoint in this group requires a JWT with role `admin`; any other authenticated role gets `403` before any state is read.

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/api/v1/admin/stats` | Admin | Platform statistics: transaction/refund aggregates plus total user and merchant counts. |
| GET | `/api/v1/admin/users` | Admin | List registered user accounts; optional `limit` query parameter. |
| GET | `/api/v1/admin/merchants` | Admin | List all merchants on the platform. |
| POST | `/api/v1/admin/merchants/{id}/status` | Admin | Set a merchant's status. Body: `{status}`. |
| GET | `/api/v1/admin/audit` | Admin | List audit-log entries; optional `limit` query parameter. |
| GET | `/api/v1/admin/settings` | Admin | Get the platform settings. |
| PUT | `/api/v1/admin/settings` | Admin | Replace the platform settings with the JSON body. |

---

## 5. Webhooks — receiving and verifying deliveries

When something happens, MiraclePay records a `WebhookEvent`:

```json
{
  "id": "evt_…",
  "merchant_id": "…",
  "event_type": "payment.completed",
  "payload": { "payment_id": "…", "amount": 5000, "currency": "BDT" },
  "created_at": "2026-11-26T00:00:00Z"
}
```

The background worker POSTs the JSON serialisation of the whole event object to the merchant's configured webhook URL (`webhook_url` on the merchant). That exact serialised body is what is signed — verify against the raw request body bytes, never against a re-serialised parse of it.

### 5.1 Delivery headers

Header names are case-insensitive on the wire; the worker sends them lowercase.

| Header | Value |
|---|---|
| `Content-Type` | `application/json` |
| `X-MiraclePay-Signature` | Hex-encoded HMAC-SHA256 of the raw body bytes, keyed by the merchant's webhook signing secret (sent as `x-miraclepay-signature`). 64 lowercase hex characters. |
| `X-MiraclePay-Event` | The event type, for example `payment.completed` (sent as `x-miraclepay-event`). |
| `X-MiraclePay-Delivery` | The delivery ID for this delivery series (sent as `x-miraclepay-delivery`). |

### 5.2 Verifying a signature

1. Read the request body as raw bytes.
2. Compute `HMAC-SHA256(secret, raw_body)` and hex-encode it (lowercase).
3. Compare it with the `X-MiraclePay-Signature` value in constant time.

The domain verifier (`verify_signature` in `crates/domain/src/webhook.rs`) accepts upper- or lowercase hex and returns `false` — never an error — for malformed signatures. You can also test a signature without receiving a delivery via `POST /api/v1/webhooks/test-signature` (§4.9).

Respond with any `2xx` status to acknowledge a delivery. Any other status, or no response, counts as a failed attempt.

### 5.3 Event types

Event types are dotted, lowercase names. The types actually emitted by v0.0.1 handlers are:

| Event type | Emitted when |
|---|---|
| `payment.created` | A payment is created via `POST /api/v1/payments`. |
| `payment.completed` | A payment is settled — in v0.0.1, when SMS verification (`POST /api/v1/sms`) exactly matches a pending/processing transaction. Its `payload` carries `transaction_id`, `merchant_id`, `gateway`, `amount_minor`, `currency`, `status`, `provider_trx_id`, `sender_number`, and `reference`. |
| `payment.refunded` | A refund is created via `POST /api/v1/transactions/{id}/refunds`. Its `payload` carries `refund_id`, `amount_minor`, `currency`, and `status` alongside the transaction identifiers. |

`payment.failed` is the other name given as an example of the naming scheme in the domain code; no v0.0.1 handler emits it, so do not depend on receiving it in this version.

### 5.4 Retries and dead letters

- A delivery is attempted at most **8 times** in total (initial attempt plus retries).
- The delay before each retry doubles from **60 seconds** — 60, 120, 240, 480, 960, 1,920, 3,840, 7,680, … — capped at **86,400 seconds (24 hours)**.
- Delivery statuses: `pending`, `delivered` (endpoint answered 2xx), `failed` (last attempt failed, retries may follow), `dead_letter` (attempts exhausted; no further automatic retries — inspect it via `GET /api/v1/webhooks/deliveries` and redeliver manually with `POST /api/v1/webhooks/events/{id}/redeliver`).

---

## 6. SMS ingest payload

Both SMS endpoints accept exactly the `IncomingSms` JSON shape from `crates/sms_verify/src/types.rs`. All fields except `sim_slot` are required; `sim_slot` is a zero-based SIM index or explicit `null` when unknown / single-SIM.

| Field | Type | Description |
|---|---|---|
| `device_id` | string | Identifier of the Android device that forwarded the message. |
| `sender` | string | Message sender as reported by the phone: a phone number or an alphanumeric sender ID (for example `bKash`), exactly as received. |
| `body` | string | Full message body, exactly as received. |
| `received_at` | string (RFC 3339 UTC) | When the message was received on the device. |
| `sim_slot` | integer \| `null` | Zero-based SIM slot the message arrived on, when the device reports it. |

Example — matching the `IncomingSms` values used in the `sms_verify` test suite:

```http
POST /api/v1/sms HTTP/1.1
Host: localhost:8080
Content-Type: application/json
x-device-key: mp_live_<32 hex characters>

{
  "device_id": "device-42",
  "sender": "bKash",
  "body": "You have received Tk 1,500.00. TrxID 9HX7K2LM1P",
  "received_at": "2023-11-14T22:13:20Z",
  "sim_slot": 1
}
```

`received_at` above is the RFC 3339 rendering of Unix timestamp `1700000000`, the value used for that test message.

### `POST /api/v1/sms` outcomes

Verification outcomes are reported in the body, not as API errors:

| Outcome | HTTP status | Body | Meaning |
|---|---|---|---|
| `verified` | 200 | `{"outcome": "verified", "transaction_id": "…"}` | Exact match. The transaction is completed, its provider transaction ID is reserved, and a `payment.completed` webhook event is queued. |
| `probable` | 200 | `{"outcome": "probable", "transaction_id": "…"}` | Likely match, reported for a human / follow-up step. Nothing is settled and the provider transaction ID is not reserved. |
| `duplicate` | 200 | `{"outcome": "duplicate"}` | This confirmation was already used; it settles nothing a second time. |
| `unmatched` | 202 | `{"outcome": "unmatched"}` | Accepted, but it matched no pending payment for this merchant. |
| `not_payment` | 202 | `{"outcome": "not_payment"}` | Accepted, but the message is not a payment SMS. |

In short: `200` means "accepted and fully dealt with"; `202` means "accepted, but this message settles nothing".
