# MiraclePay Plugins — Author Guide

MiraclePay has two plugin lanes:

1. **JS lane (in-process)** — a `main.js` run by
   [psychoscript](https://github.com/anomalyco/psychoscript)
   (MIT), a pure-Rust confined JavaScript interpreter, wrapped by
   the `miraclepay-plugin-spi` crate. psychoscript is pinned to
   git revision `bf492e12017fb25621080676ddb8cd4fa60b7a9d`; its
   types never leak into the plugin contract, so the engine can
   be swapped without breaking plugins.
2. **HTTP lane** — an out-of-process service the core calls over
   a small JSON protocol (below). Same operations, same data.

A plugin is a directory under `plugins/` (override with
`PLUGINS_DIR`):

```
plugins/<id>/
  plugin.toml   — manifest
  main.js       — entry point (JS lane only)
```

At startup the core scans the directory. A plugin whose manifest
is invalid, or whose `min_api_version` is incompatible with the
core's plugin API version (**1.0**), **refuses to load**: the
reason is logged at boot and shown in the admin API/UI. Broken
plugins never take the platform down and are never registered.

## Manifest (`plugin.toml`)

| Field | Required | Meaning |
|---|---|---|
| `id` | yes | Unique id: lowercase letters, digits, dashes. |
| `name` | yes | Human-readable name. |
| `version` | yes | Plugin version (free-form, e.g. `"1.0.0"`). |
| `kind` | yes | `gateway`, `sms_parser`, `notification`, `webhook_transformer`, or `http`. |
| `provides` | gateway/http | Gateway ids this plugin serves (e.g. `["manual_bank_transfer"]`). |
| `config_schema` | no | JSON Schema (as a TOML table) for the plugin's config; the admin UI generates a form from it. |
| `allowed_hosts` | no | Hosts `http.fetch` may reach. Default: none. Subdomains of a listed host are allowed. |
| `min_api_version` | yes | Minimum plugin API version, e.g. `"1.0"`. Same major and not newer than the core's, or the plugin refuses to load. |
| `entry` | no | Entry file name (default `main.js`). |
| `endpoint` | http kind | Base URL of the plugin service. HTTPS required (plain HTTP only for `localhost`). |
| `timeout_ms` | no | Per-call timeout, 1..=60000 (default 5000). |

## Operations by kind

A JS plugin implements an operation as a **top-level function of
the same name**, taking one JSON argument and returning JSON
data.

### Gateway (`kind = "gateway"`)

- `create_payment(req)` — `req`: `{merchant_id, amount_minor,
  currency, reference, customer_name, customer_email,
  customer_phone, callback_url, metadata}` → returns
  `{provider_ref, redirect_url?, instructions?}`.
  `provider_ref` is required; keep it stable, verification and
  refunds address the payment by it.
- `verify_payment({provider_ref})` → `{verified,
  provider_trx_id, amount_minor, sender}`. The core settles only
  when `verified` is true **and** `amount_minor` equals the
  transaction amount.
- `refund({provider_ref, amount_minor, reason})` →
  `{provider_refund_id, status}`.

Gateway plugins are resolved alongside the native registry: when
a payment names a gateway id no native adapter knows, the core
asks an enabled plugin that `provides` it. Native ids always win.

### SMS parser (`kind = "sms_parser"`)

- `parse_sms({sender, body, received_at})` →
  `{matched, amount_minor, currency, trx_id, sender_number,
  receiver_number, reference, balance_minor}`.
  Return `{matched: false}` for anything that is not your format.
  A parse only counts as a payment when both `amount_minor` and
  `trx_id` are present. Parser plugins run **after** the native
  parsers, only when those could not match the message — and a
  plugin parse settles a payment only on an exact match against a
  pending transaction, exactly like the native path.

### Notification (`kind = "notification"`)

- `notify(event)` — `event` is an object with exactly these
  keys: `event` (`"payment.completed"` or `"payment.failed"`),
  `event_id`, `merchant_id`, `transaction_id`, `gateway`,
  `amount_minor`, `currency`, `status`, `provider_trx_id`,
  `sender_number`, `reference`, `reason` (null except on
  failure), and `occurred_at` (RFC 3339). Customer PII and
  transaction metadata are deliberately excluded. The return
  value is ignored by the pipeline; returning a small summary
  object is still useful, because it is what the admin invoke
  endpoint shows when an admin calls the operation by hand.

Notification plugins fire when a payment settles or fails —
`payment.completed` (all four settlement paths: SMS native,
SMS plugin-fallback, provider webhook, and the transaction
verify endpoint) and `payment.failed` (the verify endpoint's
provider-unverified outcome — the platform has no other
payment-failure transition today). Dispatch is
**platform-global and fire-and-forget**: every enabled
notification plugin is called for every merchant's events (there
is no per-merchant subscription today — a plugin that only cares
about some merchants must filter on `event.merchant_id`
itself), the calls run off the payment path, and a plugin that
throws or exceeds its timeout is logged, counted in the
plugin's `dispatch_stats` (visible on `GET /admin/plugins`),
and skipped. A broken notification plugin can never block,
delay, or fail a payment.

### Webhook transformer (`kind = "webhook_transformer"`)

- `transform_webhook({event_type, payload, merchant_id})` →
  return a replacement payload object, or `null` to leave the
  payload unchanged.

Transformers run at the **webhook payload build point** for
`payment.completed` events (the four settlement paths listed
above): after the core has built the payload and before the
event is stored — and therefore before it is signed at
delivery. The signature covers the transformed payload, so
receivers verify against what was actually sent. Other event
types are not transformed today.

Transformation is **opt-in per merchant**: a merchant's
payloads are transformed only when their merchant settings
name a transformer in `webhook_transformer_plugin_id` (set via
`PUT /api/v1/merchants/:id/settings`; the plugin must exist
and be a loadable `webhook_transformer`). At most **one**
transformer applies per merchant — the named one; there is no
chaining. Merchants who never opt in get byte-identical
payloads to the untransformed pipeline.

Failure semantics are fail-open and strictly bounded: the call
is capped at the smaller of the plugin's manifest timeout and
2000 ms; a transformer that throws, times out, or returns a
non-object leaves the original payload in place, and the
outcome is counted in the plugin's `dispatch_stats`.
Transformation can never break webhook delivery; for a merchant who opted in, it can delay that merchant's settlement by at most the 2-second cap.

### Worked example: the two example plugins

`plugins/example-notify/` is a minimal `notification` plugin:
`notify(event)` writes the event to the plugin log, keeps a
per-event-type counter (`count:<type>`) in its KV store, and
returns `{ok, event_type, count}`. It declares no
`allowed_hosts` — it needs no network, and confinement is the
default.

`plugins/example-webhook-transform/` is a minimal
`webhook_transformer`: it returns a copy of the payload with a
`transformed_by: "example-webhook-transform"` marker and a
nested `meta: {event_type}` object added — a harmless,
inspectable example of reshaping an outbound webhook. Returning
`null` for a non-object payload shows the "leave it unchanged"
path.

## Host functions (the ONLY outside access)

A plugin gets **no filesystem, no environment variables, no
process access, and no network** except through these:

### `http.fetch(url, options?)`
`options`: `{method, headers, body}` (body: string or JSON).
Returns `{status, body}`. The URL's host must be in the
manifest's `allowed_hosts` (exact or subdomain) — checked
**before** any socket opens. Literal private/loopback IPs are
refused unless that exact IP is allowlisted.

### `config.get(key)`
Reads the plugin's **own** configuration (set by an admin in the
dashboard / admin API). Other plugins' config is unreachable.

### `kv.get(key)` / `kv.set(key, value)`
A persistent key/value store **namespaced to this plugin**
(other plugins' keys are unreachable). Values are JSON data.
Backed by the platform's Postgres persistence layer, so entries
survive restarts.

### `log.info(msg)` / `log.warn(msg)` / `log.error(msg)`
Appends to the plugin's log buffer, readable by admins
(`GET /admin/plugins/:id/logs`).

### `crypto.hmac_sha256(key, message)`
HMAC-SHA256 over UTF-8 strings, returned as lowercase hex.

## Runtime limits

Every call runs under hard limits: the manifest timeout
(default **5 s** — an infinite loop is interrupted, not
tolerated), a **32 MiB** heap cap, and a **1 MiB** output cap.

## The JavaScript subset — honestly

psychoscript is a young (2026), deliberately confined
JavaScript-like interpreter, not Node.js and not a full browser
engine. Per its `interpreter-support.md`:

- Modern JS syntax is broadly supported (classes, destructuring,
  template literals, `Map`/`Set`, promises, top-level `await` and
  `return`, `Date`, `RegExp`, `URL`, JSON). TypeScript syntax is
  **rejected**, not stripped.
- There are **no Node APIs**: no `fs`, no `process`, no timers
  beyond what host functions provide, no `require` of anything
  except globals the host itself provides. `export` is a parse
  error.
- The result boundary is JSON: functions and `undefined`
  properties vanish, `Map`/`RegExp` values serialize as `{}`,
  cyclic values throw. Keep operation inputs/outputs plain JSON.
- A single pathological regular-expression match can run long
  (the regex engine has no interrupt hook); the call timeout
  still bounds the whole call between steps.
- Built-in size guards apply (strings up to 2^24 chars, arrays
  up to 10M elements, 10k pending promises).

If your plugin needs more than this, use the HTTP lane and write
it in any language.

## HTTP lane protocol

For `kind = "http"`, the core POSTs JSON to
`{endpoint}/{operation}`:

- `POST /create_payment` — body: the gateway request object
  (same shape as the JS `create_payment` argument) → 200 with
  `{provider_ref, redirect_url?, instructions?}`.
- `POST /verify_payment` — `{provider_ref}` → `{verified,
  provider_trx_id, amount_minor, sender}`.
- `POST /refund` — `{provider_ref, amount_minor, reason}` →
  `{provider_refund_id, status}`.

Any non-2xx answer, timeout, or invalid JSON is a provider
error — the core never treats it as a successful payment.

## Worked example: the pilot gateway

`plugins/manual-bank-transfer/` ships in this repository and
runs entirely through the JS lane:

- `create_payment` stores a pending record in `kv` and returns
  bank instructions built from `config.get("account_number")`
  etc. (config set by the admin; the schema in its manifest
  drives the dashboard form).
- The merchant confirms receipt via the admin invoke operation
  `confirm_payment({provider_ref, sender})`, which flips the KV
  record to `paid`.
- `verify_payment` reports `verified: true` only for `paid`
  records, with the stored amount — so
  `POST /transactions/:id/verify` settles the payment end-to-end.

The second pilot, `plugins/brac-bank-sms/`, is an `sms_parser`
for BRAC Bank credit alerts — a format the native wallet parsers
do not cover. Its `main.js` documents the message format it
accepts and returns `{matched: false}` for everything else.

## Admin operations

Admins manage plugins via the API (and the dashboard Plugins
page): list with load errors, enable/disable (persisted),
read/replace config (persisted, applied to the running engine),
invoke arbitrary operations on JS plugins, and drain logs.
Endpoints: `GET /admin/plugins`, `POST
/admin/plugins/:id/enable|disable`, `GET|PUT
/admin/plugins/:id/config`, `POST /admin/plugins/:id/invoke`,
`GET /admin/plugins/:id/logs`.
