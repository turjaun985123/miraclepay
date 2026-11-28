# MiraclePay v0.0.4 — Stream B Spec: Payer-chosen gateway at checkout

**Seat:** B1 (Architect), analysis/spec only — no production code in this
document's delivery. **Consumers:** B2 (backend: `checkout_details` +
`submit_checkout`), B3 (frontend: checkout page picker), B4 (tests).
**Status:** Binding. Field names, validation rules, and error semantics
below are final; do not rename, reshape, or re-decide without returning
to B1/parent.

## 0. Problem and source facts (verified in tree)

Today the gateway is fixed when the transaction is created and the
payer at hosted checkout can never change it:

- `GET /api/v1/checkout/:id` — `payments::checkout_details`
  (`crates/api/src/handlers/payments.rs`) returns the transaction's
  stored `gateway` and (Stream A, landed) the scalar `receiving_number`
  for **that** gateway only. It returns **no offered list**: there is
  no `gateways` and no `receiving_numbers` key in the response.
- `POST /api/v1/checkout/:id/submit` — `payments::submit_checkout`
  accepts exactly `{ sender_number, provider_trx_id }`
  (`SubmitReq { sender_number: String, provider_trx_id: String }`).
  It requires `status == Pending` (else `409 Conflict`), stores the
  two fields, moves the transaction to `Processing`. It never touches
  `transaction.gateway`.
- The checkout page (`frontend/src/pages/checkout/[id].astro`, in
  `render()`) therefore computes
  `currentGateway = details.gateway || details.gateways?.[0] || details.methods?.[0]`
  — the fallbacks are dead code against the real API, so the creation
  gateway silently drives the "Payment method" row, the receiving
  block (`renderReceiving()` keys on `currentGateway`), and the
  `gateway` value the submit handler already spreads into its payload
  (`...(currentGateway ? { gateway: currentGateway } : {})` — the API
  client type `SubmitCheckoutData` already carries `gateway: string`;
  the backend simply ignores it today).
- The pay page (`frontend/src/pages/pay/[slug].astro`) already solves
  the same problem for payment links: a `<fieldset>` of
  `.gateway-grid` / `.gateway-card` radio cards
  (`<input type="radio" name="gateway" value="…">` + `.gateway-name` +
  `.gateway-hint`), cards hidden when not in the offered list, and a
  receiving block re-rendered on `change` from a
  `Map<gateway, number>` built out of `receiving_numbers`. Stream B
  ports that pattern to checkout; it does not invent a new one.

### 0.1 The offered-gateways rule (single source of truth)

`public_link` (`crates/api/src/handlers/payment_links.rs`) computes
the offered list exactly one way, and §1/§2 reuse it verbatim:

```rust
let registry = state.registry_for(&merchant_id);
let configured = registry.configured_ids();
let offered = if configured.is_empty() { registry.list() } else { configured };
let gateways: Vec<String> = offered.iter().map(|id| id.as_str().to_string()).collect();
```

- Merchant with ≥1 configured gateway → offered = `configured_ids()`
  (registered gateways whose adapter reports `is_configured()`), in
  canonical `GatewayId::all()` order (14 ids: `bkash, nagad, rocket,
  upay, surecash, upi, paytm, phonepe, razorpay, easypaisa, jazzcash,
  stripe, paypal, manual`).
- Merchant with none configured → offered = `registry.list()` = all
  14 supported ids (unconfigured = unrestricted). This is deliberate
  existing behaviour; Stream B does not tighten it.
- `receiving_numbers` on the link view is the intersection of that
  offered list with the merchant's `state.gateway_configs[merchant_id]`
  entries that carry a non-blank `merchant_number`, serialised with
  the **landed** struct (note: this differs from the Stream A spec's
  proposed `{gateway, receiving_number}` — the code that shipped
  wins):

```rust
// crates/api/src/handlers/payment_links.rs — ReceivingNumber
{ "gateway": String, "number": String }   // e.g. {"gateway":"bkash","number":"01700000000"}
```

Only `GatewayConfig::merchant_number` is ever exposed — never
`api_key`, `api_secret`, or `extra` (Stream A §7 rules carry over).

### 0.2 How SMS settlement actually uses `transaction.gateway` (verified)

`receive_sms` (`crates/api/src/handlers/sms.rs`) snapshots the
device-key merchant's `Pending | Processing` transactions — **no
gateway filter** — and calls
`miraclepay_sms_verify::verifier::verify_sms`, which parses the SMS
first (`parse_incoming`: gateway detected from the SMS sender/body via
`detect_bd_gateway`, then `detect_in_pk_gateway`; amount, TrxID,
sender phone extracted generically) and then
`matcher::match_transaction`. `matcher::score_candidate` awards:

| Signal | Points |
|---|---|
| `parsed.trx_id == tx.provider_trx_id` | 100 |
| amount (minor units) equal | 50 |
| `parsed.sender_number == tx.sender_number` | 30 |
| `parsed.gateway == tx.gateway` (case-insensitive) **or** receiver number == `tx.customer_phone` | 20 (once) |

`Exact` = score ≥ 80 **and** the SMS amount, when present, does not
contradict the transaction amount (Wave 8 guard). `Probable` = score
≥ 50 **and** amount **and** currency both match (currency is derived
from the *detected SMS gateway's* country, never from the
transaction). A TrxID already in `used_trx` is `Duplicate` before any
scoring. The plugin fallback in `receive_sms` parses via
`plugin_parse_sms`, which always returns `ParsedSms { gateway: None, … }`,
so the gateway signal is never awarded on that path.

**Conclusion that drives §3: `transaction.gateway` is a soft,
corroborating signal (20 points) in SMS matching — never a gate,
never a filter.** Everything else that must agree is the provider
TrxID, the amount, and the sender number, exactly as today.

## 1. `checkout_details` additions (B2)

Add exactly two top-level keys to the existing response. Every
existing key — including `gateway` and the Stream A scalar
`receiving_number` — is unchanged in name, type, and meaning.

```jsonc
{
  // … all existing keys unchanged …
  "gateway": "bkash",                       // transaction's stored gateway (unchanged)
  "receiving_number": "01700000000",        // Stream A scalar, for `gateway` (unchanged)
  "gateways": ["bkash", "nagad"],           // NEW — §1.1
  "receiving_numbers": [                    // NEW — §1.2
    { "gateway": "bkash", "number": "01700000000" },
    { "gateway": "nagad", "number": "01800000000" }
  ]
}
```

### 1.1 `gateways: Vec<String>` — always present

- Computed for `transaction.merchant_id` by the §0.1 rule, byte-for-
  byte the same computation `public_link` performs (same helper —
  see §5). Canonical `GatewayId::all()` order, lowercase `as_str()`
  spellings.
- Present on **every** status (`pending`, `processing`, `completed`,
  terminal): the page polls `checkout_details` while `processing`,
  and `render()` must not lose the keys between polls. Never omitted,
  never `null`. (In practice never `[]` either — both branches of the
  rule yield ≥1 id — but the type is a plain array and B3 must
  tolerate `[]`, §4.2.)
- Ids only. No config values, no credentials, no `enabled` flags.

### 1.2 `receiving_numbers: Vec<ReceivingNumber>` — always present

- Same shape, same computation, same ordering as the public link
  view (§0.1): reuse the `payment_links::ReceivingNumber` struct
  (`{ gateway, number }`); entries are offered gateways whose stored
  config carries a non-blank `merchant_number`, in `gateways` order;
  offered gateways without a number are **omitted** (no
  `{gateway, number: null}` entries); `[]` when none qualify.
- Relationship to the scalar: `receiving_number` remains "the number
  for `transaction.gateway`" under its existing lookup and is **not**
  redefined. One divergence is possible and is pinned, not papered
  over: if the transaction's gateway is no longer in the offered set
  (config removed/disabled after creation), the scalar can be
  non-`null` while `receiving_numbers` has no entry for that gateway.
  B3 resolves it client-side (§4.3). B2 must not "fix" the scalar to
  match the list, or vice versa.

## 2. `submit_checkout` contract (B2)

### 2.1 Request body — one new optional field

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SubmitReq {
    pub sender_number: String,
    pub provider_trx_id: String,
    /// Gateway the payer chose at checkout (Stream B). Optional for
    /// backwards compatibility: absent/null keeps the transaction's
    /// gateway.
    pub gateway: Option<String>,
}
```

```jsonc
// POST /api/v1/checkout/:id/submit
{ "sender_number": "01712345678", "provider_trx_id": "9HX7K2LMNP", "gateway": "nagad" }
```

- `gateway` absent, JSON `null`, or a string that is empty after
  trimming → **keep** `transaction.gateway` byte-identical. (Pinning
  empty-string = absent: the current checkout page already omits the
  field when it knows no gateway, and an empty string carries no
  payer intent. B4 pins this, test B-T11.)
- A non-string JSON value for `gateway` is a serde type error →
  Axum's default `400`. No custom handling.

### 2.2 Normalisation

A present, non-blank `gateway` is normalised as
`trim().to_ascii_lowercase()` — the same normalisation
`create_payment` applies when storing `transaction.gateway`
(`pay_link`, by contrast, stores its gateway verbatim; submit is the
repair point for such transactions, §2.4).

### 2.3 Validation — order and errors (all pinned)

Checks run in this exact order; the first failure wins:

1. `sender_number` empty after trim → `400 BadRequest`,
   `"sender_number must not be empty"` (existing, unchanged).
2. `provider_trx_id` empty after trim → `400 BadRequest`,
   `"provider_trx_id must not be empty"` (existing, unchanged).
3. Transaction id unknown → `404 NotFound`,
   `"transaction not found: {id}"` (existing, unchanged).
4. `transaction.status != Pending` → `409 Conflict`,
   `"transaction {id} is not pending and cannot be submitted"`
   (existing, unchanged). **Status is checked before gateway
   validation**: a late submit against a `Processing` transaction
   returns `409` even if its `gateway` is also invalid (B-T15 pins
   this). Rationale: the state conflict is the primary fact, and it
   keeps the existing check in its existing position — gateway
   validation is additive after it.
5. Gateway validation (new; only when §2.1 yields a candidate):
   let `candidate` = normalised value, `current` =
   `transaction.gateway` normalised the same way for comparison.
   - **No-op rule:** if `candidate == current`, accept unconditionally
     — even if `current` is no longer in the offered set, and even if
     it is a plugin-lane id the registry does not know. Keeping the
     creation gateway is never an error.
   - **Unknown rule:** else if `GatewayId::from_str(candidate)` is
     `None` → `400 BadRequest`, `"unknown gateway: {candidate}"`.
     (Plugin-lane gateway ids land here *as switch targets*: the
     offered set in §2.3.6 is registry-derived and never contains
     them, and a payer must not be able to switch a native checkout
     onto a plugin gateway whose receiving number/instructions this
     page cannot render. Switching *away* from a plugin-created
     transaction to an offered native gateway is allowed by the next
     rule; keeping a plugin gateway is allowed by the no-op rule.)
   - **AMENDMENT (coordinator, post-B5/B6, supersedes the Unknown
     rule's plugin clause):** B5's FINDING-1 showed the shipped
     behaviour is the consistent one — plugin-lane ids ARE valid
     switch targets, because `create_payment` accepts them and the
     picker offers them: usable plugins' `provides` ids are folded
     into the offered set on both public surfaces, and
     `submit_checkout` accepts a candidate a usable plugin provides
     (plugin ids only — an id that parses as a native `GatewayId`
     must pass the Offered rule, so a plugin can never shadow a
     native gateway into an unoffered switch; B6 F4 guard). An id
     no plugin provides and `GatewayId` does not know is still
     `400 "unknown gateway: {candidate}"`.
   - **Offered rule:** else if `candidate` is not in the offered set
     for `transaction.merchant_id` (the §0.1 list, computed for this
     transaction's merchant) → `400 BadRequest`,
     `"gateway is not offered for this merchant: {candidate}"`.
   - Otherwise accept.
6. On any `400` from step 5: **nothing is stored** — the transaction
   stays `Pending` with its previous `gateway`, `sender_number`, and
   `provider_trx_id` untouched, so the payer can correct the choice
   and resubmit (B-T13 pins the untouched state).

### 2.4 What gets stored (success path)

Under the same `transactions` write lock, atomically:

- `transaction.gateway` = the canonical accepted id
  (`GatewayId::as_str()` spelling) when a gateway candidate was
  supplied and accepted — including the no-op case, which therefore
  also canonicalises a verbatim-stored `"BKASH"` (from `pay_link`)
  to `"bkash"`. When the field was absent/null/blank, the stored
  gateway is **not rewritten at all**.
- `transaction.sender_number` = trimmed `sender_number` (existing).
- `transaction.provider_trx_id` = trimmed `provider_trx_id` (existing).
- `transaction.status` = `Processing` (existing);
  `transaction.updated_at` = `Utc::now()` (existing).
- Response unchanged: `200 { "transaction": { … } }`, with
  `transaction.gateway` reflecting the choice.
- **No new webhook event.** Submit fires none today and Stream B adds
  none; the `payment.created` event fired at creation keeps the
  creation gateway (it is a historical record of creation). Consumers
  learn the final gateway from the transaction itself and from the
  `payment.completed` webhook, whose payload is built from the
  completed transaction and therefore carries the chosen gateway
  (§3.3). A `payment.updated`-style event is out of scope (§6).

Implementation note (not a behaviour change): the offered set depends
only on `merchant_id`, so B2 computes it **before** taking the
transactions write lock (snapshot `merchant_id`/`status`/`gateway`
under a read lock, mirroring how `receive_sms` snapshots), then
re-checks `status == Pending` under the write lock before mutating —
the write-lock check is the authoritative one, exactly as today.

### 2.5 State rule

The gateway may change **only** at submit time, which by the existing
contract is only possible while `Pending`. There is no endpoint,
field, or path in this stream that changes the gateway of a
`Processing` (or later) transaction: once submitted, the chosen
gateway is as fixed as the creation gateway was before. Repeated
submits are already impossible (`409`), so a payer gets exactly one
choice point — the submit.

## 3. Settlement semantics after a switch (B2: no code change; B4: pin it)

**Stream B changes no SMS, matcher, verifier, or plugin code.** The
chosen gateway settles correctly *because* matching reads the stored
`transaction.gateway`, which §2.4 updates before the transaction
enters `Processing`. The semantics below are the contract B4 pins.

### 3.1 What must agree for an SMS to settle — unchanged

Per §0.2, in descending strength: provider TrxID (100; alone
sufficient for `Exact`, subject to the amount-contradiction guard),
amount in minor units (50; a present-but-different SMS amount vetoes
`Exact` and `Probable`), sender number (30), gateway-or-receiver
(20). `Probable` additionally requires currency agreement, where the
SMS currency comes from the *detected SMS gateway's* country.
Duplicate TrxIDs (per-merchant `used_trx`) settle nothing, ever.

### 3.2 Effect of the switch on matching

- The 20-point gateway signal now compares the SMS's detected
  gateway against the **chosen** gateway. A payer who follows the
  switched instructions produces an SMS whose detected gateway
  agrees with the stored one — the signal is earned exactly as it
  would have been had the transaction been created with that
  gateway.
- The switch can never break an otherwise-exact match: TrxID alone
  (100) and amount+sender (50+30 = 80) both reach `Exact` with zero
  gateway points. It can never create one either: the maximum swing
  is 20 points, and amount+gateway (70) is still only `Probable`.
  Gateway agreement is corroboration, not a gate — by design,
  because a provider TrxID is unique evidence for one payment while
  gateway detection from SMS text is heuristic.
- **Payer pays via the creation gateway after switching** (switches
  to Nagad on the page, actually pays via bKash): the bKash SMS
  loses the 20 gateway points but still settles `Exact` via TrxID
  (or amount+sender). This is accepted and pinned (B-T19), not an
  error path: the money arrived, the TrxID the payer submitted is
  the bKash one, and rejecting it would strand a real payment.
- **Currency edge:** switching gateways never changes
  `transaction.amount` or its currency. All BD→BD switches
  (bkash↔nagad↔rocket↔upay↔surecash) are currency-neutral. A
  cross-region switch (e.g. bkash→upi on an unconfigured merchant's
  all-14 offered list) is permitted by §2.3 — validation is about
  the offered set, not regions — but a BDT transaction can then only
  reach `Probable` from an INR SMS (currency mismatch blocks
  `Probable`; `Exact` via TrxID is unaffected). Documented sharp
  edge; not blocked in this stream (§6).

### 3.3 The differing-SMS-format case (the mandated scenario)

Creation gateway `bkash`, merchant also configured for `nagad`,
payer picks `nagad` at checkout:

1. Page re-renders Nagad instructions + Nagad receiving number (§4);
   payer sends money via Nagad and submits
   `{ sender_number, provider_trx_id: <Nagad TrxID>, gateway: "nagad" }`.
2. §2.4 stores `gateway = "nagad"`, status `Processing`.
3. The merchant's phone receives the **Nagad-format** confirmation
   SMS; the forwarder posts it to `receive_sms`. `parse_incoming`
   detects `GatewayId::Nagad` from the Nagad sender/body patterns —
   detection follows the SMS, not the transaction — and extracts the
   Nagad TrxID/amount/sender in that gateway's format.
4. Matching: TrxID 100 + amount 50 + sender 30 + gateway 20 = 200 →
   `Exact` → transaction `Completed`; the TrxID is reserved in
   `used_trx`; the `payment.completed` webhook payload carries
   `"gateway": "nagad"` (it is serialised from the completed
   transaction). B-T18 pins this end-to-end.

Two adjacent cases, both unchanged by Stream B and stated so B4 does
not mistake them for regressions:

- **SMS arrives before submit** (transaction still `Pending`): the
  matcher includes `Pending` transactions, so the SMS can settle the
  transaction under the *creation* gateway's scoring (a Nagad SMS
  against a still-bkash transaction scores 180/200 — still `Exact`).
  The payer's later submit then correctly returns `409`.
- **Plugin paths:** the SMS plugin fallback parses with
  `gateway: None`, so the chosen gateway earns no points there and
  matching is gateway-agnostic — a switched transaction settles via
  plugin parse exactly as an unswitched one. The manual verify
  endpoint (`POST /transactions/:id/verify`,
  `handlers/plugins_admin.rs`) resolves its gateway from
  `tx.gateway`, so after a switch it verifies against the **chosen**
  gateway — the desired behaviour, no change needed.

## 4. Frontend behaviour (B3) — `frontend/src/pages/checkout/[id].astro`

### 4.1 API client types (`frontend/src/lib/api.ts`)

- `CheckoutDetails`: add
  `receiving_numbers: { gateway: string; number: string }[]`
  (always present per §1.2; the page still consumes it defensively).
  `gateways?: string[]` already exists on the interface — keep it;
  the backend now actually sends it.
- `SubmitCheckoutData.gateway` already exists — no change. The
  page's local `CheckoutView` interface gains
  `receiving_numbers?: { gateway?: unknown; number?: unknown }[] | null`,
  mirroring the pay page's `PublicLinkShape` normalisation style.

### 4.2 The picker — radio cards, Pending only

- Add a `<fieldset class="field">` inside `#pending-area`, above the
  instructions block, copying the pay page's markup and classes
  verbatim: `<legend class="label">Pay with</legend>`, a
  `.gateway-grid` of `<label class="gateway-card">` cards each
  containing `<input type="radio" name="gateway" value="{id}">`,
  a `.gateway-name` span, and a `.gateway-hint` span.
- **Cards are generated from `details.gateways`, never hardcoded**
  (the pay page's six hardcoded cards are wrong here: the offered
  list can contain any of the 14 ids). Display names come from a
  page-local map for the 14 known ids (`bkash→bKash`, `nagad→Nagad`,
  `rocket→Rocket`, `upay→Upay`, `surecash→SureCash`, `upi→UPI`,
  `paytm→Paytm`, `phonepe→PhonePe`, `razorpay→Razorpay`,
  `easypaisa→Easypaisa`, `jazzcash→JazzCash`, `stripe→Stripe`,
  `paypal→PayPal`, `manual→Manual transfer`), falling back to the
  existing `prettify()`. Hints: ids in the page's existing
  `MANUAL_WALLET_GATEWAYS` set → `"Send money"`;
  `manual` → `"Bank or cash"`; all others → `"Online payment"`.
- **Visibility:** render the fieldset only when the effective card
  list (§4.3) has ≥2 entries. With ≤1 there is nothing to choose;
  the existing "Payment method" summary row already names it.
- The picker exists only in the Pending view. `processing`,
  `completed`, and terminal renders are untouched (the summary row
  keeps showing the stored gateway).

### 4.3 Pre-selection and the not-offered creation gateway

- Effective card list = `details.gateways`, **plus** the
  transaction's own `details.gateway` prepended if it is non-empty
  and not already in the list (case-insensitive comparison). That
  extra card's hint is `"Current method"`. This guarantees the
  pre-selected card always exists, and — via the §2.3 no-op rule —
  that leaving it selected always submits successfully.
- Pre-select the card whose value equals `details.gateway`
  (case-insensitive). `currentGateway` is initialised to that
  normalised (lowercase) id, replacing the current
  `details.gateway || details.gateways?.[0] || details.methods?.[0]`
  computation: the fallbacks are deleted, not kept — `gateway` is
  always the transaction's stored gateway, and the picker (not
  `gateways[0]`) is now the source of any change. If both `gateway`
  and `gateways` are somehow empty, `currentGateway = ''` and the
  page behaves exactly as today (generic instructions, no picker,
  submit omits `gateway`).

### 4.4 On change — instructions and receiving number swap

A single `change` listener on the radio group (the pay page's
pattern), which sets `currentGateway` to the selected value and
re-renders, with **no network call** (everything needed is already
in the `checkout_details` payload):

- The "Payment method" summary row (`#checkout-gateway`) shows the
  selected gateway's display name.
- The receiving block is re-rendered by generalising the existing
  `renderReceiving()`: build a `Map<string, string>` from
  `details.receiving_numbers` (lowercase gateway → number, the pay
  page's normalisation), then — for the §1.2 divergence — if the
  scalar `details.receiving_number` is non-`null` and
  `details.gateway` is set, set/overwrite the map entry for
  `details.gateway` with the scalar. The block then shows the
  selected gateway's entry: label `"Send money to this {Gateway}
  number"` + copyable number, or — for a selected gateway in
  `MANUAL_WALLET_GATEWAYS` (or no gateway) with no entry — the
  existing bilingual "not published / contact the merchant" note;
  for a non-manual gateway with no entry and no redirect, the block
  hides, exactly as `renderReceiving()` decides today, but keyed on
  the **selected** gateway instead of the creation gateway.
- The generic instructions paragraph names the selected gateway:
  when no creation-time `details.instructions` string is present
  (the real API sends none today), the paragraph text is
  `"Send the amount shown above to the merchant using {Gateway},
  then enter your wallet number and the transaction ID from that
  payment below."` with `{Gateway}` = the selected display name,
  updated on every change. When a creation-time `instructions`
  string *is* present it is left untouched on switch (it may contain
  merchant-specific text B3 cannot safely rewrite) — only the
  receiving block and method row swap in that case.
- The language toggle re-runs the same re-render (as it already does
  for `renderReceiving()`), so the label/note follow en/bn for the
  **selected** gateway.
- Sender/TrxID inputs are **not** cleared or re-validated on switch,
  and any visible form error is cleared. `render()` must not reset
  the payer's selection: it derives the checked card from a
  page-level `selectedGateway` variable (initialised per §4.3,
  updated by the change listener), not by re-reading
  `details.gateway` on every render — status polling calls
  `render()` repeatedly and must preserve an in-progress choice.

### 4.5 On submit

- The payload always includes the current selection:
  `{ sender_number, provider_trx_id, transaction_id: provider_trx_id,
  gateway: selectedGateway }` — i.e. the existing conditional spread
  becomes unconditional whenever `selectedGateway` is non-empty
  (which §4.3 makes the normal case, including "kept the creation
  gateway": sending it explicitly is what exercises the §2.3 no-op
  rule and canonicalises case). When `selectedGateway === ''`, omit
  `gateway` as today.
- Error handling is the existing path, unchanged: a `400` from §2.3
  (unknown / not offered) or a `409` surfaces the server's message
  via `showFormError()`, the button is re-enabled with its original
  label, and — for a `400` — the transaction is still `Pending`
  (§2.3.6), so correcting the radio choice and resubmitting works
  without a page reload. On success the existing submitted-area +
  polling flow runs.

## 5. Implementation shape for B2 (one reusable unit)

`public_link` already contains the offered-list and
receiving-numbers computations inline. B2 extracts them into one
shared helper (e.g. in `handlers/payment_links.rs`, re-exported, or
a small shared module) with the shape:

```rust
/// Offered gateway ids for a merchant (configured, else all
/// supported) and their payer-safe receiving numbers — the single
/// computation behind PublicLinkView and checkout_details.
pub(crate) fn offered_gateways_and_numbers(
    state: &AppState,
    merchant_id: &str,
) -> (Vec<String>, Vec<ReceivingNumber>)
```

`public_link` is refactored to call it (behaviour byte-identical —
Stream A's tests are the regression net) and `checkout_details`
calls it for §1. Two copies of the §0.1 rule are a spec violation:
the lists must not be able to drift. B2 also updates, in the same
pass: `crates/api/src/openapi.rs` (checkout GET response fields;
submit request body gains optional `gateway`) and `docs/API.md`
(checkout rows: submit body becomes
`{sender_number, provider_trx_id, gateway?}`; the §4.6.1-style field
table gains `gateways` / `receiving_numbers` for
`GET /api/v1/checkout/{id}`).

## 6. Out of scope / explicitly unchanged

- No matcher, verifier, parser, plugin, or `receive_sms` changes
  (§3 is semantics-pinning only).
- No gateway change after `Pending`; no new endpoint; no
  `payment.updated` webhook; the `payment.created` payload keeps the
  creation gateway.
- `create_payment` and `pay_link` creation-time gateway handling is
  untouched (including `pay_link` storing the gateway verbatim and
  not validating it against the offered set — pre-existing behaviour,
  not Stream B's to fix; §2.4's canonicalisation only repairs the
  stored value when a payer submits with an explicit gateway).
- The Stream A scalar `receiving_number` lookup logic in
  `checkout_details` is untouched.
- Cross-region offered sets are not restricted (§3.2 currency edge
  is documented, not blocked).
- No per-gateway `instructions` field is added to `checkout_details`:
  the page's instructions are generic-plus-receiving-number today,
  and §4.4's gateway-name interpolation covers the switch. (The
  adapters' `initiated.instructions` from creation time remain a
  `create_payment` response concern only.)

## 7. Test matrix for B4

Style: `crates/api/tests/` integration tests over the real router
(follow `crates/api/tests/receiving_number.rs` helpers:
`register_and_login`, `create_merchant`, `configure_gateway`,
`create_payment`, `get_checkout`, device-key + `POST /api/v1/sms`
settle flow). Constants: bkash number `01700000000`, nagad number
`01800000000`, customer phone/sender as in the existing file, amount
BDT 1,250.00 (125000 minor) unless a test says otherwise. A
"multi-gateway merchant" = bkash + nagad both configured with
numbers and credentials.

### 7.1 `checkout_details` additions

- **B-T1 — Multi-gateway merchant lists both.** Transaction created
  with `gateway: "bkash"` for a multi-gateway merchant →
  `GET /api/v1/checkout/:id` 200; `gateways == ["bkash", "nagad"]`
  (canonical order); `gateway == "bkash"`;
  `receiving_numbers == [{"gateway":"bkash","number":"01700000000"},
  {"gateway":"nagad","number":"01800000000"}]`;
  scalar `receiving_number == "01700000000"`.
- **B-T2 — Unconfigured merchant offers all 14.** Merchant with no
  gateway configs, transaction via public link paid with
  `gateway: "bkash"` → `gateways` is exactly the 14 ids in
  `GatewayId::all()` order; `receiving_numbers == []`; keys present.
- **B-T3 — Configured-only restriction.** Merchant with **only**
  bkash configured → `gateways == ["bkash"]`,
  `receiving_numbers` has the bkash entry only.
- **B-T4 — Offered gateway without a number is omitted.** Multi-
  gateway merchant plus `manual` configured with **no**
  `merchant_number` → `gateways` contains `manual`;
  `receiving_numbers` contains no `manual` entry (bkash/nagad only).
- **B-T5 — Keys present on non-Pending views.** After a successful
  submit (status `processing`), `GET` checkout still carries
  `gateways` and `receiving_numbers` with the same values as B-T1.
- **B-T6 — No-secret audit extended.** With sentinel
  `api_key`/`api_secret`/`extra` values configured on bkash+nagad:
  the checkout body contains none of the sentinels and none of the
  keys `api_key`, `api_secret`, `extra`, `merchant_number`; every
  `receiving_numbers[i]` object has exactly the keys
  `gateway`, `number`. (Extends Stream A's audit to the new fields.)

### 7.2 `submit_checkout` — backwards compatibility

- **B-T7 — Absent gateway keeps creation gateway.** Pending bkash
  transaction; submit `{sender_number, provider_trx_id}` (today's
  body, byte-for-byte) → 200, `transaction.gateway == "bkash"`,
  status `processing`, sender/TrxID stored.
- **B-T8 — `gateway: null` keeps creation gateway.** Same assertions
  as B-T7 with `"gateway": null` in the body.
- **B-T9 — Explicit same gateway.** Submit with
  `"gateway": "bkash"` → 200, gateway `bkash`.
- **B-T10 — Same gateway, shouting case.** Transaction created via
  public link with `gateway: "BKASH"` (stored verbatim); submit with
  `"gateway": "BKASH"` → 200 via the no-op rule, and the stored
  gateway is canonicalised: read-back `transaction.gateway ==
  "bkash"` and `GET` checkout `gateway == "bkash"`.
- **B-T11 — Empty-string gateway = absent.** Submit with
  `"gateway": "  "` → 200, gateway unchanged (`bkash`).

### 7.3 `submit_checkout` — the switch

- **B-T12 — Switch bkash → nagad (multi-gateway merchant).** Submit
  `{sender_number, provider_trx_id: "NGDTRX1234", gateway: "nagad"}`
  → 200; `transaction.gateway == "nagad"`, status `processing`,
  sender/TrxID stored. Follow-up `GET` checkout: `gateway ==
  "nagad"`, scalar `receiving_number == "01800000000"` (the scalar
  follows the stored gateway), `gateways`/`receiving_numbers`
  unchanged from B-T1.
- **B-T13 — Known but not offered → 400, nothing stored.** Merchant
  with only bkash configured; submit `gateway: "stripe"` → 400,
  body `error` contains `"gateway is not offered for this merchant:
  stripe"` and `code == "bad_request"`. Read back via `GET`
  checkout: status still `pending`, `gateway == "bkash"`. Then a
  corrected submit (`gateway: "bkash"`, fresh TrxID) succeeds —
  proving the failed submit stored neither sender nor TrxID and did
  not consume the one submit.
- **B-T14 — Unknown gateway → 400.** Multi-gateway merchant; submit
  `gateway: "bkash-personal"` → 400, `error` contains
  `"unknown gateway: bkash-personal"`; transaction still `pending`
  with gateway `bkash`.
- **B-T15 — State beats validation.** Submit once successfully
  (→ `processing`); submit again with `gateway: "bkash-personal"`
  → **409** (not 400), gateway unchanged. Also: second submit with a
  *valid different* gateway (`nagad`) → 409, gateway remains the
  first submit's choice — the §2.5 one-choice-point rule.
- **B-T16 — Switch on an unconfigured merchant.** No configs at all;
  link-created bkash transaction; submit `gateway: "rocket"` → 200
  (all-14 offered rule), stored gateway `rocket`; `GET` checkout
  scalar `receiving_number == null`.
- **B-T17 — Unknown transaction.** Submit with a valid gateway to a
  nonexistent id → 404 (existing behaviour, with the new field
  present in the body).

### 7.4 Settlement on the chosen gateway

- **B-T18 — MANDATED: settle via SMS on the CHOSEN gateway when it
  differs from the creation gateway.** Multi-gateway merchant.
  Create via public link (or `create_payment`) with
  `gateway: "bkash"`. Submit checkout with
  `{sender_number: <customer>, provider_trx_id: <Nagad TrxID>,
  gateway: "nagad"}` → 200, `processing`, gateway `nagad`. Create a
  device API key (`payments_write`) and `POST /api/v1/sms` a
  Nagad-format confirmation SMS (sender `Nagad`; body carrying the
  same amount, the same TrxID, and the customer as sender — the
  Nagad analogue of the existing file's bKash settle fixture) →
  200 `{"outcome": "verified", "transaction_id": <id>}`. Read back:
  transaction `completed`, `gateway == "nagad"`,
  `provider_trx_id` == the Nagad TrxID. (This is the exact case
  that fails silently today: the payer paid via Nagad while the
  transaction claimed bKash.)
- **B-T19 — Creation-gateway SMS still settles after a switch
  (soft-signal pin).** As B-T18, but after switching to `nagad`,
  forward the **bKash**-format SMS carrying the submitted TrxID,
  amount, and sender → still `verified`/`completed` (TrxID-exact;
  gateway points lost, §3.2). Pins that gateway is corroborating,
  not gating.
- **B-T20 — Amount+sender-only SMS on the chosen gateway.** As
  B-T18, but the transaction was submitted with a TrxID the SMS
  does **not** carry, and the Nagad SMS carries amount + sender
  only (no TrxID) → `verified` (50 + 30 + 20 = 100 ≥ 80, gateway
  points earned against the *chosen* gateway).
- **B-T21 — Duplicate protection unchanged.** After B-T18 settles,
  re-forward the identical Nagad SMS → `{"outcome": "duplicate"}`,
  no state change.
- **B-T22 — SMS before submit settles under creation gateway.**
  Pending bkash transaction (multi-gateway merchant); forward the
  Nagad SMS (amount + sender + its own TrxID) **before** any submit
  → `verified`, transaction `completed` with gateway still
  `bkash`. A subsequent checkout submit (any gateway) → 409.

### 7.5 Frontend checklist (B3 self-check; B4 verifies by inspection)

- **B-T23 — Picker rendering.** Multi-gateway checkout: two radio
  cards, bkash pre-selected, Nagad receiving number shown after
  selecting the Nagad card and bkash's after re-selecting bkash;
  single-gateway merchant: no fieldset, page otherwise identical to
  today; submit payload (network log) contains
  `"gateway": "nagad"` after a switch and `"gateway": "bkash"`
  when untouched.
