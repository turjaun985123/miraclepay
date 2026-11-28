# MiraclePay v0.0.4 — Stream D Spec: Plugin Dispatch for `notification` and `webhook_transformer`

Seat D1 (Architect), spec only — no production code in this document's
delivery. Builders D2 (dispatch core), D3 (settings + SMS call sites),
D4 (remaining call sites + docs), D5 (tests) work directly from this
document. Every name, field, bound and file location below was read
from the code, not inferred:

- `crates/api/src/plugins.rs` — `LoadedPlugin { dir_name, manifest,
  engine, enabled, load_error }`, `usable() = enabled &&
  load_error.is_none() && manifest.is_some()`; existing dispatch:
  `plugin_initiate` / `plugin_verify` / `plugin_refund` (via private
  `call_gateway`, JS + HTTP lanes) and `plugin_parse_sms` (JS only,
  collects usable `SmsParser` engines, one `spawn_blocking`, iterates).
- `crates/plugin_spi/src/manifest.rs` — `PluginKind::{Gateway,
  SmsParser, Notification, WebhookTransformer, Http}` (serde
  `snake_case`); `PluginManifest::timeout_ms()` = manifest
  `timeout_ms` or **5000**; manifest validation bounds `timeout_ms`
  to `1..=60000`.
- `crates/plugin_spi/src/engine.rs` — `PluginEngine::call(operation,
  &Value) -> Result<Value, PluginError>`; operation must be a
  top-level JS function of that name; `PluginError::NoSuchOperation`
  when absent; every call runs under psychoscript `Limits`:
  manifest timeout, 32 MiB heap, 1 MiB output. Host functions are
  exactly `http.fetch` (allowlist-gated), `config.get`, `kv.get/set`
  (per-plugin namespace), `log.*`, `crypto.hmac_sha256`.
- `crates/api/src/handlers/sms.rs` — the two `payment.completed`
  settle paths (§2.1, sites S1/S2).
- `crates/api/src/handlers/provider_webhooks.rs` —
  `settle_by_reference` (Stripe/PayPal/bKash settle, via sync
  `AppState::emit_webhook` in `crates/api/src/state.rs:321`).
- `crates/api/src/handlers/plugins_admin.rs` — admin invoke
  (`POST /admin/plugins/:id/invoke`, `spawn_blocking` +
  `engine.call`) and `verify_transaction`
  (`POST /transactions/:id/verify`, third `payment.completed` site).
- `crates/domain/src/settings.rs` — `MerchantSettings` (serde-defaulted
  optional fields are the established pattern: `brand_name`,
  `payout_schedule`, …); `crates/api/src/state.rs` —
  `merchant_settings: Arc<RwLock<HashMap<String, MerchantSettings>>>`;
  `crates/api/src/handlers/merchants.rs` —
  `default_merchant_settings()` + `GET|PUT
  /api/v1/merchants/:id/settings` (routes.rs:163) which serialise the
  whole struct, no DTO.
- `crates/domain/src/webhook.rs` — `WebhookEvent { id, merchant_id,
  event_type, payload: Value, created_at }`; events are stored by
  pushing to `state.webhook_events` and delivered from the stored
  payload, so transforming the payload **before** the push is the
  only transformation point that affects delivery.
- `docs/plugins.md` — "Notification / webhook transformer" section
  currently says *Reserved kinds … wiring them into the dispatch
  pipeline is future work*. D4 replaces that section (§7).

## 0. Decisions at a glance (binding)

| # | Decision |
|---|---|
| D-1 | Notification operation is **`notify`**, argument key for the event name is **`event`**. |
| D-2 | Notification dispatch is **platform-global broadcast**: every usable JS-lane `notification` plugin receives every event. **No per-merchant opt-in in v0.0.4** (§2.4). |
| D-3 | Notification dispatch is **fire-and-forget**: detached `tokio::spawn`; the settle path never awaits a plugin, never sees its error, and cannot be failed or measurably delayed by it (§2.3). |
| D-4 | Webhook transformer operation is **`transform_webhook`**, argument `{event_type, payload, merchant_id}`; return `null` = unchanged, a JSON **object** = replacement payload, anything else = invalid → original (§3.1). |
| D-5 | Transformer opt-in is **per-merchant, at most one plugin**, via the new `MerchantSettings` field **`webhook_transformer_plugin_id: Option<String>`** (§3.2). No chaining, ever, in this wave. |
| D-6 | Transformer call is **awaited but hard-bounded**: effective bound `min(manifest timeout_ms, 2000 ms)`; any failure mode → original payload, logged + counted (§3.4). |
| D-7 | v0.0.4 creates **no new `TxStatus` transitions**. A repo-wide grep verifies **zero** production assignments to `TxStatus::Failed` or `TxStatus::Expired` today (only `seed.rs` fixtures and the status-string parser in `payments.rs`). `payment.failed` notification fires at the one concrete provider-verdict point that exists (§2.2, F1); it does not mutate the transaction. |
| D-8 | Both kinds are **JS lane only** in v0.0.4. `PluginKind` is single-valued, so a `kind = "http"` plugin cannot also be a notification/transformer plugin; HTTP-lane dispatch for these kinds is out of scope and must not be half-built. |
| D-9 | Host functions, confinement and egress are **unchanged** (§4). No new globals, no new network path. |
| D-10 | No DB migration. The settings field rides the existing JSON persistence of `merchant_settings`; dispatch stats are in-memory only (§5). |

---

## 1. Shared dispatch core (D2 — `crates/api/src/plugins.rs`)

### 1.1 Stats (the "counted" half of "logged + counted")

Add to `AppState` (`crates/api/src/state.rs`, next to `plugins` /
`plugin_enabled`; initialise in `AppState::new`):

```rust
/// Per-plugin dispatch counters for the Stream D plugin kinds
/// (notification + webhook transformer). In-memory only (§5).
pub plugin_dispatch_stats:
    Arc<RwLock<HashMap<String, crate::plugins::DispatchStats>>>,
```

Add to `crates/api/src/plugins.rs`:

```rust
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct DispatchStats {
    pub notify_attempted: u64,
    pub notify_succeeded: u64,
    pub notify_failed: u64,
    pub notify_timed_out: u64,
    pub notify_skipped_no_op: u64,
    pub transform_attempted: u64,
    pub transform_succeeded: u64,
    pub transform_failed: u64,
    pub transform_timed_out: u64,
    pub transform_skipped_no_op: u64,
}
```

Rules:

- Counters are keyed by **plugin id**, incremented under the write
  lock at the moment the outcome is known (for notify: inside the
  detached task). A missing key means all-zero.
- Exposure: `GET /admin/plugins` (the existing list handler in
  `plugins_admin.rs`) gains one additive key per plugin object,
  `"dispatch_stats"`, with the struct above serialised (all-zero
  object when the plugin has no entry). No new endpoint.
- Stats are **not** persisted (§5). Restart resets them — same
  status as the plugin log buffers, acceptable for v0.0.4 telemetry.

### 1.2 Logging convention

The API crate's existing ad-hoc convention is `eprintln!` (see
`tracing_note` in `handlers/transactions.rs`). Use exactly that; do
not add a logging dependency. Formats (single line each):

- `miraclepay: plugin notify {plugin_id} failed for {event} tx {transaction_id}: {error}`
- `miraclepay: plugin notify {plugin_id} timed out for {event} tx {transaction_id}`
- `miraclepay: plugin notify {plugin_id} does not implement notify (event {event} tx {transaction_id})`
- `miraclepay: plugin transform_webhook {plugin_id} failed for merchant {merchant_id} event {event_type}: {error} — original payload kept`
- `miraclepay: plugin transform_webhook {plugin_id} timed out for merchant {merchant_id} event {event_type} — original payload kept`
- `miraclepay: plugin transform_webhook {plugin_id} returned a non-object payload for merchant {merchant_id} event {event_type} — original payload kept`
- `miraclepay: webhook transformer {plugin_id} for merchant {merchant_id} is not usable — original payload kept`

**Never log a payload** (original or transformed) — payloads carry
sender numbers and references. Log ids and event types only.

### 1.3 Collecting target plugins (both kinds)

Mirror `plugin_parse_sms`'s collection pattern, with one correction:
`state.plugins` is a `HashMap`, so iteration order is
nondeterministic. Both dispatchers **sort the collected plugins by
plugin id** before dispatch so behaviour (and test assertions) are
deterministic.

A plugin is a dispatch target iff **all** of:

- `plugin.usable()` is true,
- `plugin.kind() == Some(PluginKind::Notification)` (resp.
  `WebhookTransformer`),
- `plugin.engine` is `Some` (JS lane; D-8).

Snapshot `(plugin_id, Arc<PluginEngine>, manifest.timeout_ms())`
tuples under the read lock, then drop the lock before any call —
exactly as `call_gateway` / `plugin_parse_sms` already do.

---

## 2. Notification dispatch (D2 core, D3/D4 call sites)

### 2.1 The operation and the event shape

Operation name: **`notify`** — a top-level JS function taking one
JSON argument, e.g. `function notify(event) { … }`.

Argument (exact field names; built from the settled `Transaction`
clone that each settle site already holds):

```json
{
  "event": "payment.completed",
  "event_id": "evt_…",
  "merchant_id": "mch_…",
  "transaction_id": "txn_…",
  "gateway": "bkash",
  "amount_minor": 50000,
  "currency": "BDT",
  "status": "completed",
  "provider_trx_id": "9HX…" ,
  "sender_number": "01…",
  "reference": "order-123",
  "reason": null,
  "occurred_at": "2026-11-28T14:03:22.123Z"
}
```

Field rules:

- `event` — `"payment.completed"` or `"payment.failed"` (the only
  two values v0.0.4 emits). Note the key is `event`, **not**
  `event_type` — the notification argument is its own shape, not a
  `WebhookEvent`.
- `event_id` — correlation id. At sites that also build a
  `WebhookEvent` (S1, S2, S4), it is **that event's `id`** (the
  `ids::webhook_event_id()` value), so an operator can trace
  notification ↔ webhook delivery. At F1 (no webhook event exists)
  it is a fresh `ids::webhook_event_id()` used for correlation only;
  nothing is stored under it.
- `merchant_id`, `transaction_id`, `gateway` — copied from the
  transaction (`completed.merchant_id`, `completed.id`,
  `completed.gateway`).
- `amount_minor` — `tx.amount.amount_minor` (i64, minor units).
- `currency` — `tx.amount.currency.code()` (e.g. `"BDT"`), matching
  the SMS webhook payloads.
- `status` — the transaction's status **after** the transition,
  serialised as the existing `TxStatus` serde form
  (`"completed"` at settle sites; the unchanged current status,
  e.g. `"processing"`, at F1).
- `provider_trx_id`, `sender_number`, `reference` — the
  transaction's `Option` fields, serialised as string-or-`null`
  (exactly as the existing `json!` webhook payloads already emit
  them).
- `reason` — `null` for `payment.completed`; `"provider_unverified"`
  at F1 (§2.2). No other values in v0.0.4.
- `occurred_at` — `Utc::now()` at the firing site, RFC 3339 (the
  `serde_json` serialisation of `DateTime<Utc>`, as elsewhere).

**Deliberately excluded** (data-minimisation, §2.4): `customer_email`,
`customer_phone`, `customer_name`, `metadata`. The transaction's
`metadata` is merchant-supplied free-form JSON and must not be
broadcast to platform plugins.

Builder contract (D2): one constructor in `plugins.rs` so every site
builds the identical shape —

```rust
/// Build the `notify` argument for a transaction outcome.
pub fn notification_arg(
    event: &str,               // "payment.completed" | "payment.failed"
    event_id: &str,
    tx: &miraclepay_domain::transaction::Transaction,
    reason: Option<&str>,
) -> Value
```

Return value of `notify`: **ignored entirely** — any JSON value,
including `null`, counts as success. There is no acknowledgement
protocol in v0.0.4.

### 2.2 Firing points

**`payment.completed` — required sites (all four concrete settle
sites in the codebase):**

| Site | Location | Notes |
|---|---|---|
| S1 | `handlers/sms.rs`, `receive_sms`, `VerificationOutcome::Verified` branch | After the `payment.completed` `WebhookEvent` push, using `completed` and that event's id. |
| S2 | `handlers/sms.rs`, `plugin_fallback` settle tail | Same position: after the webhook push (`via: "sms_plugin_parser"` payload), using `completed` and that event's id. **This is the second SMS settle path the stream brief names; S1+S2 are the minimum deliverable.** |
| S3 | `handlers/provider_webhooks.rs`, `settle_by_reference` | After `state.emit_webhook(...)`, using the `settled` clone. Covers Stripe `checkout.session.completed`, PayPal `PAYMENT.CAPTURE.COMPLETED`, bKash callback success. The duplicate-completion early return (`tx.status == Completed` → `Some(id)`) must **not** fire — the payment did not complete *now*. Only the fall-through settle fires, exactly once. |
| S4 | `handlers/plugins_admin.rs`, `verify_transaction` settle tail | Inside the existing `if let Some(completed) = completed` block, after the webhook push, using that event's id. |

S3/S4 are in scope (not optional polish): without them a notification
plugin silently misses every gateway-webhook and manual-verify
settlement — the exact "pretends to deliver" failure `docs/plugins.md`
warns against. Each is a single call next to an existing webhook push,
with the settled clone already in hand.

**`payment.failed` — the one concrete point (D-7):**

| Site | Location | Notes |
|---|---|---|
| F1 | `handlers/plugins_admin.rs`, `verify_transaction`, the `if !result.verified` early return | The provider for an identified transaction has returned an explicit negative verdict. Fire `notify` with `event: "payment.failed"`, `reason: "provider_unverified"`, `status` = the transaction's **unchanged** current status. **Do not change `tx.status`** — introducing a `Failed` transition is a lifecycle decision outside Stream D. The response body (`{"ok": true, "status": "unverified", "verified": false}`) is unchanged. |

**Explicitly excluded, with reasons (do not wire these):**

- `bkash_callback` with `status != "success"`
  (`provider_webhooks.rs:297`): the handler resolves **no**
  transaction and mutates nothing on that path (it returns before
  `settle_by_reference`). Firing would require inventing a
  reference→transaction lookup; deferred beyond v0.0.4.
- `create_payment` gateway-initiate errors (`handlers/payments.rs`):
  the transaction is inserted into state only **after** a successful
  initiate, so on failure there is no persisted transaction to
  notify about.
- `settle_refund` with outcome `failed` (`handlers/transactions.rs`):
  that is a **refund** failure; it already emits a `refund.failed`
  webhook. It is not `payment.failed` and must not be conflated.
- SMS `ProbableMatch` / `Duplicate` / `Unmatched` / `NotPayment`
  outcomes: nothing settled and nothing failed — no event.
- Expiry: no code expires transactions today (D-7), so there is no
  `payment.failed`-on-expiry point to wire.

### 2.3 Fire-and-forget mechanics (D2)

Exact signature in `crates/api/src/plugins.rs`:

```rust
/// Fire-and-forget notification dispatch. Collects targets and
/// spawns the delivery task; returns as soon as the task is
/// spawned. Never returns an error.
pub async fn dispatch_notification(state: &AppState, arg: Value)
```

Mechanics:

1. Collect targets per §1.3 (kind `Notification`). If empty, return
   — zero spawned work on the hot path.
2. `AppState` is `#[derive(Clone)]` and every field is an `Arc`
   (plus cloneable scalars), so the detached task owns what it
   needs: the collected `(plugin_id, Arc<PluginEngine>, timeout_ms)`
   vec, the `arg: Value`, and a clone of
   `state.plugin_dispatch_stats`.
3. `tokio::spawn` one detached task. Inside it, **for each plugin in
   sorted order, sequentially**:
   - increment `notify_attempted`;
   - run the call as `plugin_parse_sms` / `call_gateway` do —
     `tokio::task::spawn_blocking(move || engine.call("notify",
     &arg))` — wrapped in
     `tokio::time::timeout(Duration::from_millis(timeout_ms) +
     Duration::from_millis(2_000), join_handle)`.
     The engine's own `Limits` already interrupt the interpreter at
     `timeout_ms`; the outer `+2 s` is a backstop against a wedged
     blocking thread, not the primary bound;
   - outcome mapping: `Ok(Ok(_))` → `notify_succeeded`;
     `Ok(Err(PluginError::NoSuchOperation(_)))` →
     `notify_skipped_no_op` + the no-op log line (a plugin of kind
     `notification` that does not implement `notify` is a
     misconfiguration to surface in logs/stats, not a delivery
     failure); `Ok(Err(e))` → `notify_failed` + failure log line;
     outer-timeout elapsed or `JoinError` → `notify_timed_out` +
     timeout log line.
   - Sequential-per-plugin inside one task is deliberate: worst-case
     detached-task lifetime is `Σ(timeout_ms + 2 s)` over installed
     notification plugins, and it costs the request path **nothing**.
     Do not parallelise in v0.0.4 (ordering of side effects stays
     predictable, and plugin count is operator-controlled and small).
4. Call sites invoke it as a plain statement after the webhook push:

   ```rust
   crate::plugins::dispatch_notification(&state,
       crate::plugins::notification_arg("payment.completed",
           &event_id, &completed, None)).await;
   ```

   The `.await` returns once the task is spawned (the function does
   no plugin work inline). **No call site may await delivery, branch
   on an outcome, or hold any state lock across this call** — at
   S1/S2/S4 the `transactions` / `webhook_events` write guards are
   already dropped at the push point; keep it that way (build the
   arg from the `completed` clone, never under a lock).

Guarantee to state in code docs and in `docs/plugins.md`: a
notification plugin that hangs, throws, or returns garbage **cannot
delay the settle response beyond the spawn, cannot fail the
settlement, and cannot prevent the webhook event from being
stored** (the webhook push happens first at every site).

### 2.4 Who receives it — platform-global broadcast (D-2), and why

**Decision: every usable `notification` plugin receives every
event, for every merchant. There is no per-merchant opt-in for
notifications in v0.0.4.**

Justification:

- Plugins are **platform-level** today, end to end: they live in one
  `PLUGINS_DIR`, are loaded once into `state.plugins`, and are
  installed, enabled/disabled, configured and invoked **only by
  platform admins** (`plugins_admin.rs` handlers all call
  `require_admin`). A notification plugin is therefore part of the
  operator's own infrastructure (e.g. "post settlements to the ops
  chat", "mirror events to our monitoring sink") — the same trust
  tier as the platform's own log pipeline, not a third party.
- There is no merchant→plugin association anywhere in the current
  model (no field on `Merchant`, `MerchantSettings`, or the manifest
  expresses one). Inventing opt-in for notifications would need a
  *list-valued* association (a merchant may want several channels)
  plus enablement UI semantics — a bigger contract than this wave.
  The transformer (§3) gets the per-merchant association because it
  *mutates merchant data* (their webhook payload), which demands
  merchant consent; a read-only notification tap does not.
- What a notification plugin sees is exactly the §2.1 shape:
  transaction id, merchant id, gateway, amount, currency, status,
  provider trx id, sender number, reference, timestamps. This is
  the same field set the merchant's own `payment.completed` webhook
  already carries, **minus** customer PII and `metadata` (excluded
  deliberately). Sender number is retained because the SMS-settled
  flows have no other party identifier and ops notifications for
  those flows are useless without it.
- Residual risk, stated honestly for the release notes: an installed
  notification plugin observes **all** merchants' payment events and
  can egress them via its manifest `allowed_hosts` through the
  existing `http.fetch`. That is acceptable for v0.0.4 **only**
  because installation and enablement are admin-only platform
  decisions. If plugins ever become merchant-installable, this
  decision must be revisited and replaced with per-merchant opt-in
  before that ships.

---

## 3. Webhook transformer (D3 settings, D2 helper, D3/D4 call sites)

### 3.1 The operation and its contract

Operation name: **`transform_webhook`** — top-level JS function,
one JSON argument:

```json
{
  "event_type": "payment.completed",
  "merchant_id": "mch_…",
  "payload": { "transaction_id": "txn_…", "amount_minor": 50000, "…": "…" }
}
```

- `event_type` — the `WebhookEvent.event_type` being built
  (`"payment.completed"` at every v0.0.4 application point, §3.3).
- `merchant_id` — the event's merchant (also inside `payload` at
  most sites; repeated at top level so a plugin can route without
  knowing each event's payload shape).
- `payload` — the payload exactly as the core built it, pre-transform.

Return contract (exact):

| Return | Meaning |
|---|---|
| `null` (JS `null` / absent return → `Value::Null`) | Unchanged — store the original payload. Counts as `transform_succeeded`. |
| A JSON **object** `{…}` | **Replaces** the payload wholesale (not merged). The stored `WebhookEvent.payload` is this object, verbatim. Counts as `transform_succeeded`. |
| Any other JSON value (string, number, bool, array) | Invalid → original payload, `transform_failed` + non-object log line. |
| Throw / runtime error | Original payload, `transform_failed` + failure log line. |
| `PluginError::NoSuchOperation` | Original payload, `transform_skipped_no_op` + warn log (merchant opted into a plugin that does not implement the operation — a misconfiguration the operator must be able to see). |
| Timeout (§3.4) | Original payload, `transform_timed_out` + timeout log line. |

The plugin's return is **not** schema-validated beyond
"is an object": the merchant chose this plugin to shape *their*
payload, and the core does not know the downstream consumer's
schema. The 1 MiB engine output cap still applies.

### 3.2 Opt-in: the settings field (D3 — domain + merchants handler)

In `crates/domain/src/settings.rs`, `MerchantSettings` gains
(exact name, exact type, placed after `payout_day`):

```rust
/// Webhook transformer plugin (Stream D): the id of a
/// `webhook_transformer` plugin whose `transform_webhook`
/// operation rewrites this merchant's `payment.completed`
/// webhook payloads before they are stored. `None` (and absence
/// in old persisted records) means no transformation.
#[serde(default)]
pub webhook_transformer_plugin_id: Option<String>,
```

Consequences and required companion changes:

- **Backwards compatibility**: `#[serde(default)]` follows the
  `brand_name` / `payout_schedule` precedent — persisted settings
  JSON written before this wave deserialises with `None`. No
  migration, no persistence-version bump (§5).
- Every `MerchantSettings` literal in the repo must gain
  `webhook_transformer_plugin_id: None` or the crate will not
  compile — the known sites are the two test helpers in
  `settings.rs` itself and `default_merchant_settings()` in
  `handlers/merchants.rs` (grep `MerchantSettings {` for the rest).
- **Domain validation** (`validate_merchant_settings`): when
  `Some(id)`, require the plugin-id charset already used by the
  manifest validator — non-empty, all chars
  `ascii_lowercase | ascii_digit | '-'` — else push
  `"webhook_transformer_plugin_id must be a plugin id (lowercase letters, digits and dashes)"`.
  The domain layer must **not** check existence or kind (it cannot
  see plugins).
- **API-layer validation** (`update_merchant_settings` in
  `handlers/merchants.rs`, after domain validation, before insert):
  when `Some(id)`, look the id up in `state.plugins`; reject with
  `ApiError::BadRequest(format!("webhook_transformer_plugin_id does
  not name a webhook_transformer plugin: {id}"))` unless a plugin
  with that id exists, has `kind() == Some(PluginKind::
  WebhookTransformer)`, and has `load_error.is_none()`.
  `enabled` is deliberately **not** required at PUT time — an admin
  may temporarily disable the plugin; dispatch (§3.3) skips a
  disabled plugin and keeps the original payload. This gives
  merchants a settings value that survives plugin maintenance
  instead of a PUT that starts failing.
- **Settings API exposure**: none needed beyond the field itself.
  `GET /api/v1/merchants/:id/settings` returns the stored (or
  default) `MerchantSettings` as JSON and `PUT` accepts the same
  struct, so the field appears as `"webhook_transformer_plugin_id":
  "my-transformer" | null` in both directions automatically. The
  dashboard settings UI is **out of scope** for Stream D (follow-up;
  the API is the contract this wave).

### 3.3 Application points and the helper (D2 helper; D3/D4 sites)

Helper, exact signature, in `crates/api/src/plugins.rs`:

```rust
/// Run the merchant's opted-in webhook transformer, if any, over
/// a payload about to be stored in a WebhookEvent. Returns the
/// payload to store — the original on no-opt-in, skip, timeout,
/// error, or invalid return. Awaited but hard-bounded (§3.4).
pub async fn apply_webhook_transformer(
    state: &AppState,
    merchant_id: &str,
    event_type: &str,
    payload: Value,
) -> Value
```

Behaviour inside the helper, in order:

1. Read the merchant's settings:
   `state.merchant_settings.read().await.get(merchant_id)` →
   `webhook_transformer_plugin_id`. Absent settings record or
   `None` → return `payload` unchanged (no stats, no log — this is
   the overwhelmingly common path and must stay free).
2. Resolve the named plugin per §1.3 with kind
   `WebhookTransformer`. Not found / not usable / no engine →
   the not-usable log line, return original. (No counter — the
   plugin may not exist to key stats on; if it exists but is
   disabled, still no counter increment beyond the log line.)
3. Increment `transform_attempted`; call per §3.4; map the outcome
   per the §3.1 table; return replacement-or-original.

Application points — **exactly the four `payment.completed` sites
of §2.2 (S1–S4)**, transforming the payload *before* the
`WebhookEvent` is constructed/pushed:

- S1 (`sms.rs` Verified): hoist the `json!({...})` payload into a
  `let payload = …;`, then
  `let payload = crate::plugins::apply_webhook_transformer(&state,
  &merchant_id, "payment.completed", payload).await;` before the
  `webhook_events.push(WebhookEvent { …, payload, … })`.
- S2 (`sms.rs` `plugin_fallback`): identical treatment; the payload
  (including `"via": "sms_plugin_parser"`) is what the transformer
  receives.
- S3 (`provider_webhooks.rs` `settle_by_reference`): `emit_webhook`
  is synchronous and non-blocking by design (`state.rs`) and must
  **not** be changed. Instead: build the `json!` payload in
  `settle_by_reference` (an async fn), await
  `apply_webhook_transformer`, then pass the result to the existing
  `state.emit_webhook(&settled.merchant_id, "payment.completed",
  payload)` call.
- S4 (`plugins_admin.rs` `verify_transaction`): same hoist-and-await
  as S1 before the push.

**Scope bound — do not apply the transformer anywhere else in
v0.0.4**: not to `payment.created` (`payments.rs`), not to
`payment.refunded` / `refund.succeeded` / `refund.failed`
(`transactions.rs`), not to dispute/settlement/subscription events
(`emit_webhook` callers in `disputes.rs`, `settlements.rs`,
`subscriptions.rs`). The helper is generic over `event_type` so a
later wave can widen coverage deliberately; silently transforming
other event types now would change merchant-visible payloads the
merchant never opted into for those types. D5 pins this bound with
a test (§6, T-14).

**Interaction with notification at the same site**: independent.
The notification argument (§2.1) is built from the **transaction**,
never from the transformed payload, so notification plugins always
see the canonical field set even when the merchant's webhook
payload was rewritten. Site order is fixed: mutate tx → build
payload → transform → push webhook event → fire notification.

### 3.4 Failure semantics and the bound (D-6)

The transform is on the settle path (the merchant opted in and the
stored payload depends on the result), so it is awaited — but
strictly bounded:

- Execution mirrors the other lanes: `spawn_blocking(move ||
  engine.call("transform_webhook", &args))`.
- The await is wrapped in
  `tokio::time::timeout(Duration::from_millis(effective_ms), …)`
  where **`effective_ms = min(manifest.timeout_ms(), 2_000)`** —
  a hard cap constant in `plugins.rs`:
  `const TRANSFORM_TIMEOUT_CAP_MS: u64 = 2_000;`
- Note honestly in code docs: the engine's own `Limits` timeout is
  the *manifest* value (up to 60 s), so when the outer 2 s cap fires
  first, the detached blocking thread keeps running until the
  engine interrupts it at the manifest timeout; the **caller** is
  released at 2 s with the original payload. Worst-case added
  settle latency for an opted-in merchant is therefore **2 s**;
  for every non-opted-in merchant it is **zero** (helper returns at
  step 1). The 5 s manifest default is *not* the bound — the cap
  exists precisely because 5 s (let alone 60 s) is too long to add
  to a settle response.
- Every failure mode (timeout, runtime error, `NoSuchOperation`,
  non-object return, `JoinError`) → **original payload**, logged
  and counted per §3.1/§1.2. The transform can never fail a
  settlement, never drop the webhook event, and never store a
  partial/invalid payload.

### 3.5 Ordering — at most one transformer (D-5)

There is no ordering problem to solve, by construction: the
settings field is a single `Option<String>`, so **at most one
transformer can apply to a merchant**, it is the one the merchant
named, and it runs exactly once per event. Chaining multiple
transformers was rejected: chained rewrites make payload provenance
undebuggable (no intermediate is stored), make failure attribution
ambiguous, and no merchant requirement for chaining exists in this
wave. If a merchant needs two transformations, one plugin can do
both — plugins can call `http.fetch` and keep state in `kv`.

---

## 4. Confinement notes

- **Host functions unchanged.** `notify` and `transform_webhook`
  run on the same `PluginEngine` with the same five host namespaces
  (`http`, `config`, `kv`, `log`, `crypto`). No new globals, no new
  host functions, no SPI change: `crates/plugin_spi` is **not
  modified** by Stream D. (`PluginKind::Notification` /
  `WebhookTransformer` and the `call` API already exist.)
- **No new egress.** A notification or transformer plugin can reach
  the network only through the pre-existing `http.fetch`, gated by
  its own manifest `allowed_hosts`, with the pre-existing
  private/loopback literal-IP refusal. Stream D adds no HTTP client,
  no webhook of its own, and no direct socket use. The HTTP plugin
  lane is untouched (D-8).
- **Same runtime limits** apply automatically: manifest timeout
  (plus §3.4's tighter caller-side cap for transforms), 32 MiB heap,
  1 MiB output.
- **Isolation between plugins** is unchanged: `config.get` reads
  only the plugin's own config; `kv` is namespaced by plugin id via
  the existing `StateKvStore`. One plugin's `notify` cannot see
  another's config/KV, and the dispatch loop hands each engine only
  its own call.
- **Trust statement** (goes into `docs/plugins.md`, D4): dispatch
  does not sandbox *intent* — an installed plugin is operator-trusted
  code. §2.4's broadcast decision rests on admin-only installation;
  §3's transformer rests on explicit per-merchant opt-in.

## 5. Persistence implications

- **`merchant_settings`**: persisted today as JSON entries in the
  `merchant_settings` snapshot map (`crates/api/src/persist.rs`
  `map_entries` / `apply_map`). The new field serialises into that
  JSON automatically and old snapshots load via `#[serde(default)]`.
  **No migration, no new snapshot key, no DB schema change.**
- **`webhook_events`**: the *transformed* payload is what is stored
  and later delivered; the original payload is **not retained**
  anywhere. Accepted consequence: after the fact you cannot diff
  what the transformer changed. Mitigation is the §1.2 logging
  (plugin id + merchant + event type on failure; nothing on success)
  plus the admin invoke endpoint, which lets an operator replay
  `transform_webhook` against a sample payload manually. Do not add
  payload logging to "fix" this (§1.2).
- **Dispatch stats**: in-memory only (§1.1), reset on restart, not
  added to `persist.rs`. Deliberate: they are operational telemetry
  of the same standing as plugin log buffers.
- **Plugin KV/config**: unchanged paths. A notification plugin that
  calls `kv.set` writes through the existing `StateKvStore` into
  `state.plugin_kv`, which the existing snapshot cadence persists
  under the `plugin_kv` key — no Stream D work, but D5's shape test
  (§6, T-3) relies on reading it back.
- **No new persisted types.** `DispatchStats` is runtime state only;
  `WebhookEvent`, `Transaction`, and the plugin manifest format are
  all unchanged.

## 6. Test matrix for D5

Conventions: test plugins are built in-test the way existing plugin
tests do — a `PluginManifest` + JS source via
`PluginEngine::from_source`, or fixture plugin directories under a
temp `PLUGINS_DIR` scanned by `scan_plugins`, matching whichever
pattern the nearest existing API/plugin tests use. SMS settle tests
reuse the existing `receive_sms` test harness/fixtures in
`sms.rs`'s test module (device key + pending transaction). All
timing assertions use the generous bounds stated — CI machines are
slow; the bound being proven is "bounded and non-blocking", not a
precise duration.

**Notification:**

- **T-1 (shape, S1)** — a `notification` plugin whose `notify`
  does `kv.set("last", JSON.stringify(event))`. Settle via the
  native SMS path. Assert the stored KV JSON parses to exactly the
  §2.1 field set: `event == "payment.completed"`, `event_id` equals
  the stored `WebhookEvent.id`, `merchant_id` / `transaction_id` /
  `gateway` / `amount_minor` / `currency` / `status ==
  "completed"` / `provider_trx_id` / `sender_number` / `reference`
  match the transaction, `reason == null`, `occurred_at` parses as
  RFC 3339. Assert **no** `customer_email`, `customer_phone`, or
  `metadata` key is present.
- **T-2 (shape, S2)** — same assertion through the SMS
  **plugin-parser** settle path (`plugin_fallback`), proving the
  second SMS path fires.
- **T-3 (broadcast + sorting)** — two usable `notification` plugins
  (ids chosen so HashMap order would differ from sorted order) both
  receive the event; a third `notification` plugin that is
  **disabled**, and a fourth with a load error, receive nothing
  (their KV markers stay absent).
- **T-4 (throwing plugin)** — `notify` throws. SMS settle still
  returns `200 {"outcome":"verified",…}`, the transaction is
  `Completed`, the webhook event is stored, and
  `notify_failed == 1` for that plugin (poll stats up to ~2 s —
  dispatch is detached).
- **T-5 (hanging plugin — the settlement-isolation test)** —
  `notify` is `while (true) {}`, manifest `timeout_ms = 300`.
  Assert: `receive_sms` returns `200 verified` with **elapsed <
  5 s** (in practice it returns at spawn time); transaction
  `Completed`; webhook event stored; and within a ~3 s poll,
  stats show `notify_failed + notify_timed_out >= 1` for the
  plugin (the engine timeout surfaces as a runtime error →
  `notify_failed`; the outer backstop → `notify_timed_out`; either
  proves the bound). The test itself must complete in < 10 s.
- **T-6 (missing op)** — a `notification` plugin with no `notify`
  function: settle succeeds; `notify_skipped_no_op == 1`,
  `notify_failed == 0`.
- **T-7 (F1, payment.failed)** — plugin-gateway transaction;
  `verify_payment` returns `{"verified": false}`; call
  `POST /transactions/:id/verify`. Assert response is the existing
  unverified body, transaction status is **unchanged** (not
  `Failed`), and the plugin received `event == "payment.failed"`,
  `reason == "provider_unverified"`.
- **T-8 (S3 dedupe)** — settle via `settle_by_reference`, then call
  it again with the same reference (provider-retry simulation):
  exactly **one** notification is delivered.
- **T-9 (no plugins)** — with zero notification plugins installed,
  SMS settle behaves byte-identically to today (response, stored
  webhook payload).

**Webhook transformer:**

- **T-10 (replacement, S1)** — merchant opts in
  (`webhook_transformer_plugin_id` set via `PUT
  /merchants/:id/settings`); `transform_webhook` returns
  `{transformed: true, tid: payload.transaction_id}`. SMS settle;
  assert the **stored** `WebhookEvent.payload` equals the returned
  object exactly, and `transform_succeeded == 1`.
- **T-11 (null = unchanged)** — transformer returns `null`; stored
  payload equals the pre-D payload for S1 byte-for-byte (including
  all existing fields).
- **T-12 (invalid returns)** — three sub-cases (string return,
  array return, throw): stored payload is the original in each;
  stats show `transform_failed` increments; settle response is
  unaffected.
- **T-13 (hanging transformer — bounded-await test)** —
  `transform_webhook` is `while (true) {}`, manifest
  `timeout_ms = 300`. Opted-in merchant settles via SMS. Assert:
  settle returns `200 verified`, **elapsed < 10 s** (the §3.4 cap
  is 2 s; the engine's 300 ms interrupt should normally win —
  either path proves boundedness), stored payload is the
  **original**, and `transform_failed + transform_timed_out >= 1`.
- **T-14 (scope pin)** — with a transformer opted in that replaces
  every payload it sees: `payment.created` (from `create_payment`)
  and `payment.refunded` events are stored **untransformed**; only
  `payment.completed` is transformed.
- **T-15 (no opt-in / wrong plugin)** — (a) merchant with
  `webhook_transformer_plugin_id == None`: a usable transformer
  plugin is installed but its KV marker proves it was never
  called. (b) opted-in plugin **disabled after** the PUT: settle
  stores the original payload. (c) opted-in plugin lacks
  `transform_webhook`: original payload +
  `transform_skipped_no_op == 1`.
- **T-16 (settings validation)** — `PUT` settings with
  `webhook_transformer_plugin_id` naming (i) a nonexistent id,
  (ii) a `gateway`-kind plugin id, (iii) a load-errored transformer
  → each `400`; (iv) a usable transformer → `200` and `GET` returns
  the id; (v) `null` clears it (`200`, subsequent settle
  untransformed). Domain unit tests in `settings.rs`: id with
  uppercase/space fails `validate_merchant_settings`; **a JSON
  settings record without the field deserialises with `None`**
  (back-compat pin), and `default_merchant_settings()` yields
  `None`.
- **T-17 (S2 + S3 + S4 coverage)** — transform applied on the
  plugin-parser SMS settle (payload incl. `"via"` reaches the
  transformer), on `settle_by_reference`, and on
  `verify_transaction` settle; one test each, replacement asserted
  in the stored event.
- **T-18 (interaction)** — merchant opted into a transformer **and**
  a notification plugin installed: the notification argument still
  carries the canonical §2.1 fields (built from the transaction)
  even though the stored webhook payload was replaced.
- **T-19 (regression)** — admin invoke
  (`POST /admin/plugins/:id/invoke`) still works for both kinds
  (`notify` and `transform_webhook` invoked manually return their
  results), and the existing gateway/SMS plugin test suites pass
  unchanged.

## 7. Builder assignments & docs

- **D2** — §1 (stats type + `AppState` field + list-endpoint key),
  §2.3 (`dispatch_notification`, `notification_arg`), §3.3 helper
  (`apply_webhook_transformer` + `TRANSFORM_TIMEOUT_CAP_MS`), all in
  `crates/api/src/plugins.rs` (+ the `state.rs` field).
- **D3** — §3.2 (domain field + validation + merchants-handler
  default/API validation) and the S1/S2 call sites in `sms.rs`
  (transform-then-push, then notify).
- **D4** — S3 (`provider_webhooks.rs`), S4 + F1 (`plugins_admin.rs`),
  and `docs/plugins.md`: replace the "Reserved kinds" paragraph with
  real sections for `notify` (§2.1 shape, §2.3 guarantee, §2.4 trust
  note) and `transform_webhook` (§3.1 contract, §3.2 opt-in, §3.4
  bound), mirroring the existing gateway/SMS operation docs.
- **D5** — §6 test matrix, T-1…T-19.

**Explicit non-goals for all builders**: no `plugin_spi` changes; no
new `TxStatus` transitions; no HTTP-lane dispatch for these kinds;
no per-merchant notification opt-in; no transformer application
beyond S1–S4; no dashboard UI; no payload logging.
