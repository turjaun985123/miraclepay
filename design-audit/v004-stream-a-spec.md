# MiraclePay v0.0.4 — Stream A Spec: Payer-visible receiving number

**Seat:** A1 (Architect), analysis/spec only. **Consumers:** A2 (backend:
`checkout_details`), A3 (frontend + `PublicLinkView` consumption), A4 (tests).
**Status:** Binding. Field names, shapes, and rules below are final; do not
rename or reshape without returning to A1/parent.

## 0. Problem and source facts (verified in tree)

The payer is never told where to send money on the public surfaces:

- `payments::checkout_details` — `GET /api/v1/checkout/:id`
  (`crates/api/src/handlers/payments.rs`) returns 14 top-level keys
  (transaction/amount/gateway/status/merchant/branding) and no receiving
  number, even though the transaction's gateway is fixed
  (`transaction.gateway`).
- `PublicLinkView` — `GET /api/v1/payment-links/public/:slug`
  (`crates/api/src/handlers/payment_links.rs`) returns the offered gateway
  **ids only** (`gateways: Vec<String>`); the payer picks a gateway on the
  pay page *before* any transaction exists, so a per-transaction field
  cannot serve this surface.

The number already exists and is already payer-facing elsewhere:

- `GatewayConfig.merchant_number: Option<String>`
  (`crates/gateways/src/types.rs`), stored per merchant in
  `state.gateway_configs: HashMap<merchant_id, Vec<GatewayConfig>>`
  (`crates/api/src/state.rs`).
- Every number-based adapter embeds it verbatim in its `initiate`
  instructions (e.g. `configured_initiate_instructions_contain_merchant_number`
  tests), and the authenticated `GatewayConfigView`
  (`crates/api/src/handlers/gateways.rs`) returns `merchant_number`
  **unmasked** to the owner (only `api_secret` is masked to `"***"`).
  Exposing it to the payer is therefore the designed purpose of the field,
  not a new disclosure class. **Do not mask it** on the public surfaces —
  a masked number cannot be paid to.
- Seed (`crates/api/src/seed.rs`): `seed_demo` creates **no**
  `gateway_configs`. The demo merchant therefore yields the no-number
  behaviour (§4) on both surfaces. No seed change in this stream.

## 1. Decision summary

| Surface | New field(s) | Shape |
|---|---|---|
| `checkout_details` | `receiving_number`, `receiving_number_available` | scalar `string \| null` + `boolean`, **always present** |
| `PublicLinkView` | `receiving_numbers` | array of `{ gateway, receiving_number }`, **always present** (may be `[]`) |
| `PayLinkResponse`, `pay_link`, `submit_checkout`, `create_payment`, authenticated payment views | — | **unchanged** |

Names: the public field is `receiving_number`, never `merchant_number`
(that name stays internal to `GatewayConfig` / owner views).

## 2. Exposure rule (the only rule A2/A3 implement)

### 2.1 Gateway classification — by how the adapter is configured

**EXPOSED set (11) — number-based, manual / SMS-verified adapters.**
For these, `merchant_number` *is* the pay-to address and
`is_configured()` is defined as "enabled config with a usable
`merchant_number`":

`bkash`, `nagad`, `rocket`, `upay`, `surecash` (BD mobile money),
`upi`, `paytm`, `phonepe` (IN manual, `crates/gateways/src/in_manual.rs`),
`easypaisa`, `jazzcash` (PK manual, `crates/gateways/src/pk_manual.rs`),
`manual` (generic manual gateway).

**NEVER set (3) — API-credential adapters.** `stripe`, `paypal`,
`razorpay` are configured by `api_key` + `api_secret`
(`merchant_number: None` in their own test fixtures) and pay via
redirect/hosted flow. Their `merchant_number` is meaningless: never
expose it **even if a stored config carries a non-empty value** (test
T3 pins this with a sentinel).

**Everything else:** unknown gateway ids and plugin-lane gateway ids
have no entry in `gateway_configs` and are never exposed.

### 2.2 Predicate — a number is exposed for `(merchant_id, gateway)` iff ALL hold

1. `gateway` parses via `GatewayId::from_str` (case-insensitive, trims —
   required because `pay_link` stores `req.gateway` verbatim, unlike
   `create_payment` which lowercases) **and** is in the EXPOSED set.
2. A config for that gateway exists in
   `state.gateway_configs[merchant_id]`. If several exist, the **last**
   one wins (mirrors `GatewayRegistry::from_configs` / upsert semantics).
3. That config has `enabled == true`.
4. `merchant_number` is `Some`, and non-empty after trimming.
5. Identifier validity, reusing the adapters' own validators (a number
   the adapter itself would reject as `InvalidConfig` must not be shown
   to a payer):
   - `upi`: `miraclepay_gateways::in_manual::valid_vpa(v)`
   - `paytm`, `phonepe`: `in_manual::valid_in_mobile(v) || valid_vpa(v)`
   - `easypaisa`, `jazzcash`: `miraclepay_gateways::pk_manual::valid_pk_mobile(v)`
   - `bkash`, `nagad`, `rocket`, `upay`, `surecash`, `manual`: no shape
     check beyond rule 4 (matches their adapters).

The exposed value is the stored string with leading/trailing whitespace
trimmed; interior characters are preserved verbatim (PK/IN validators
tolerate interior spaces/dashes, and the payer must copy the exact
address, e.g. a UPI VPA `shop@okhdfcbank`).

### 2.3 Anti-fallback rules (forbidden shortcuts)

- Read the number **only** from `state.gateway_configs`. Never fall back
  to `Merchant.phone`, `Merchant.email`, branding/settings, or
  `extra["instructions"]`.
- A2 must read `state.gateway_configs.read().await` directly in the
  handler (or via one shared helper, §6). Do **not** derive this from
  `registry_for(...)`: the registry does not expose raw numbers, and its
  `try_read` fallback silently degrades to `all_unconfigured()`.

## 3. `checkout_details` (A2) — selected gateway = `transaction.gateway`

The transaction already fixes the gateway, so a scalar is correct here.
Add exactly two top-level keys; all 14 existing keys are unchanged.

```jsonc
// bkash, configured (integration-test merchant, number 01700000000)
{
  "transaction_id": "txn_…",
  "amount": "BDT 1,250.00", "amount_display": "BDT 1,250.00",
  "amount_minor": 125000, "currency": "BDT",
  "gateway": "bkash", "status": "pending",
  "merchant": "Dhaka Integration Shop", "merchant_name": "Dhaka Integration Shop",
  "business_name": "Dhaka Integration Shop",
  "branding": { "display_name": null, "brand_name": null, "brand_color": null,
                 "logo_url": null, "checkout_message": null },
  "brand_color": null, "logo_url": null, "checkout_message": null,
  "receiving_number": "01700000000",          // NEW
  "receiving_number_available": true          // NEW
}
```

No-number cases (§4) return `"receiving_number": null,
"receiving_number_available": false` — e.g. the same transaction with
`gateway: "stripe"`, or a bkash transaction whose merchant never
configured bkash. Invariant, always:
`receiving_number_available == (receiving_number != null)`.

## 4. No-number behaviour (both surfaces)

- HTTP status and all other fields are unchanged (200, never an error).
- Keys are **never omitted** and the number is **never** `""`: explicit
  `null` + flag (checkout) / omission from the list (link view, §5).
- UI contract for A3: when the selected gateway has no number, hide any
  copy-number control and show an "ask the merchant for the
  {gateway display name} number to send to" notice. The payer can still
  submit sender number + TrxID as today.

## 5. `PublicLinkView` (A2 backend shape, A3 consumption)

The payer selects the gateway before a transaction exists, so the view
carries one entry **per offered gateway that passes §2.2**. Add exactly
one top-level field:

```rust
pub struct PublicLinkView {
    // … existing fields unchanged …
    /// Payer-safe receiving numbers, one per offered gateway that has
    /// a usable one, in the same order as `gateways`.
    pub receiving_numbers: Vec<PublicReceivingNumber>,
}

#[derive(Debug, Serialize)]
pub struct PublicReceivingNumber {
    /// Gateway id, same spelling as entries of `gateways`.
    pub gateway: String,
    /// The merchant's receiving number/address for that gateway,
    /// verbatim (outer-trimmed). Never a credential.
    pub receiving_number: String,
}
```

```jsonc
// merchant configured bkash + nagad (+ manual enabled, no number)
{
  "title": "Integration Store Pay", "description": "One integration widget",
  "amount": { "amount_minor": 125000, "currency": "BDT" },
  "merchant_name": "Dhaka Integration Shop",
  "collect_phone": true, "collect_email": false,
  "gateways": ["bkash", "nagad", "manual"],
  "receiving_numbers": [                              // NEW
    { "gateway": "bkash", "receiving_number": "01700000000" },
    { "gateway": "nagad",  "receiving_number": "01800000000" }
  ],
  "branding": { "display_name": null, "brand_color": null,
                "logo_url": null, "checkout_message": null }
}
```

Rules:

- **List of objects, not a map** — decided: it preserves `gateways`
  ordering, serializes as a typed struct (no free-form map keys), and
  leaves room for a future per-gateway field without a shape break.
- Entries are the **intersection** of `gateways` (the offered list the
  handler already computes) and §2.2-passing configs, in `gateways`
  order. A gateway with no usable number is **omitted** — no
  `{ gateway, receiving_number: null }` entries.
- **No global availability flag** on this view — decided: with several
  gateways a single boolean is ambiguous. Per-gateway availability is
  exactly "an entry with this `gateway` exists in `receiving_numbers`";
  A3 looks up the payer's current selection in the list (§4 notice on
  a miss).
- Merchant with no configs (incl. the seed merchant): `gateways` keeps
  its current all-gateways fallback behaviour, `receiving_numbers: []`.

## 6. Implementation shape for A2 (one reusable unit)

One shared helper used by both handlers, e.g. in
`crates/api/src/handlers/gateways.rs` (or a small module it re-exports):

```rust
/// Payer-safe receiving number for one gateway from a merchant's
/// stored configs, per the v0.0.4 Stream A spec §2.2.
pub(crate) fn receiving_number(
    configs: &[GatewayConfig],
    gateway: GatewayId,
) -> Option<String>
```

`checkout_details`: parse `transaction.gateway` with
`GatewayId::from_str`; on `None`, or helper returning `None`, emit
§4 null/false. `public_link`: iterate the already-computed offered
`gateways`, keep those the helper resolves. Never serialize a
`GatewayConfig` (or any part of it besides the resolved string) into a
public response.

## 7. Field-audit allowlists and forbidden fields

### 7.1 `PublicLinkView` — extend the existing audit in
`crates/api/tests/integration.rs::public_payment_link_flow_settles_via_sms`

- Top-level `allowed`: existing 8 (`title`, `description`, `amount`,
  `merchant_name`, `collect_phone`, `collect_email`, `gateways`,
  `branding`) **+ `receiving_numbers`** = 9.
- New nested audit: every `receiving_numbers[i]` object has **exactly**
  the keys `gateway`, `receiving_number`.
- Forbidden-substring list: existing (`merchant_id`, `owner`,
  `api_key`, `api_secret`, `secret`, `password`) **+ `merchant_number`,
  `extra`, `sandbox`**. (None of these substrings occurs in any
  legitimate value of the new shape.)

### 7.2 `checkout_details` — new audit (none exists today; A4 adds one)

- Top-level allowlist is exactly the 16 keys of §3 (14 existing +
  `receiving_number` + `receiving_number_available`).
- Same forbidden substrings as §7.1.

### 7.3 Forbidden on both public surfaces — keys and values

Keys: `api_key`, `api_secret`, `extra`, `sandbox`, `enabled`,
`configured`, `merchant_number` (as a key name), `merchant_id`,
`owner_user_id`, `password`/`password_hash`, `totp_secret`,
`webhook_url`. Values: a stored config's `api_key`, `api_secret`, or
any `extra` value must not appear anywhere in the body, nested or
otherwise (pinned by sentinel test T11). The authenticated masking
approach (`api_secret → "***"`) is **not** reused here: public
responses must not contain the field at all.

## 8. Test cases A4 must implement

All in `crates/api/tests/integration.rs` style (real router, `send`
helper; configure gateways via `PUT /api/v1/merchants/{id}/gateways`
like `configure_bkash`). Constants: bkash number `01700000000`
(`MERCHANT_BKASH_NUMBER`, already defined).

- **T1 — Link-flow audit extension.** In
  `public_payment_link_flow_settles_via_sms`: apply §7.1 allowlists;
  assert `receiving_numbers == [{"gateway":"bkash","receiving_number":"01700000000"}]`;
  in the final checkout read-back assert §7.2 allowlist and
  `receiving_number == "01700000000"`,
  `receiving_number_available == true`.
- **T2 — Checkout happy path (standalone).** bkash merchant → pay via
  link (or create payment) → `GET /api/v1/checkout/:id` unauthenticated
  → 200, number + `true`, `gateway == "bkash"`.
- **T3 — Credential gateway never exposes.** Configure `stripe` with
  `api_key`/`api_secret` set **and** `merchant_number: "STRIPE-NUM-SENTINEL"`;
  create a stripe transaction → checkout: `receiving_number == null`,
  `available == false`, body nowhere contains `STRIPE-NUM-SENTINEL`.
  Link view for that merchant: no `stripe` entry in `receiving_numbers`.
- **T4 — No config at all.** Merchant with zero gateway configs; link
  paid with `gateway: "bkash"` (pay_link does not require a config) →
  checkout 200 with null/false; public link view:
  `receiving_numbers == []`.
- **T5 — Disabled config.** bkash config `enabled: false` with a number
  → checkout null/false; link `receiving_numbers == []`.
- **T6 — Missing/blank number.** bkash enabled with
  `merchant_number: null`, and separately `"  "` → null/false both times.
- **T7 — Multi-gateway link mapping.** Configure bkash `01700000000`,
  nagad `01800000000`, manual enabled with **no** number → public view:
  `gateways` contains all three; `receiving_numbers` is exactly the
  bkash and nagad entries, correctly paired, in `gateways` order;
  manual omitted.
- **T8 — Malformed identifiers not exposed.** easypaisa enabled with
  `01712345678` (BD shape, invalid PK) → no easypaisa entry / checkout
  null. upi enabled with `merchant_number: "shop"` → not exposed; upi
  with `"shop@okhdfcbank"` → exposed verbatim.
- **T9 — Unknown/plugin gateway id.** Link paid with
  `gateway: "bkash-personal"` (unparseable) → checkout 200 (not 500),
  null/false.
- **T10 — Case-insensitive selection.** Link paid with
  `gateway: "BKASH"` (stored verbatim by `pay_link`) → checkout exposes
  the bkash number.
- **T11 — Sentinel leak audit.** bkash config with
  `api_key: "AK-SENTINEL"`, `api_secret: "AS-SENTINEL"`,
  `extra: {"instructions": "EXTRA-SENTINEL"}` → neither the public link
  body nor the checkout body contains any sentinel or the keys
  `api_key`, `api_secret`, `extra`.
- **T12 — Manual gateway both ways.** Manual enabled **with** a number
  → checkout for a manual transaction exposes it. Manual enabled
  **without** a number (still "configured" per its adapter) → null/false,
  proving availability tracks the *number*, not configured-ness.
- **T13 — Helper unit tests (A2/A4, in-crate).** §2.2 predicate table:
  EXPOSED vs NEVER set, disabled, blank, duplicate-config last-wins,
  outer-trim of the returned value.

## 9. Out of scope / unchanged

`PayLinkResponse` (the payer already saw the number on the link view;
the follow-up checkout view carries it), `create_payment`'s
`initiated.instructions` (already embeds the number for number-based
adapters), authenticated gateway views and their secret masking, the
seed, and all frontend rendering beyond consuming §3/§5 (A3). This
change is additive-only: existing clients ignoring the new fields are
unaffected.
