# MiraclePay v0.0.4 — Stream C Spec: Merchant Dashboard for Disputes & Payouts (Settlements)

Seat C1 (Architect), analysis/spec only. Builders C2 (API gaps), C3 (Disputes page),
C4 (Payouts page) work directly from this document. All field names, enum values,
limits and error codes below were read from the code, not inferred:

- `crates/api/src/handlers/disputes.rs`, `crates/api/src/handlers/settlements.rs`
- `crates/api/src/routes.rs` (route table)
- `crates/domain/src/dispute.rs`, `crates/domain/src/settlement.rs`,
  `crates/domain/src/money.rs`, `crates/domain/src/settings.rs`,
  `crates/domain/src/stats.rs` (`compute_balance`)
- `crates/worker/src/settlement_job.rs` (`collect_candidates` / `build_settlements` / `mark_paid`)
- `crates/api/src/pagination.rs`, `crates/api/src/dto.rs` (`ApiError`)
- Frontend model page: `frontend/src/pages/dashboard/transactions.astro`;
  shared: `frontend/src/lib/api.ts`, `frontend/src/lib/ui.ts` (`statusBadge`),
  `frontend/src/components/{DashboardNav,PageHeader,EmptyState}.astro`

Terminology used in the UI: the backend calls a payout record a **Settlement**.
The merchant-facing page is titled **Payouts** (route `/dashboard/payouts`) and
uses the word "settlement" in body copy where precision matters ("each payout is
a settlement for one period"). Do not invent a separate "Payout" entity — there
is none in the API.

---

## 1. Shared conventions (both pages, all builders)

### 1.1 Auth & scoping model (verified in both handlers)

- Every endpoint below requires the normal Bearer auth (`AuthUser` extractor;
  unauthenticated → `401 {error, code:"unauthorized", status:401}`).
- `may_access(state, auth, merchant_id)` is identical in both handlers:
  `auth.is_admin()` (token role exactly `"admin"`) OR the caller is the owner
  (`merchant.owner_user_id == auth.user_id`) of that merchant.
- Consequences for the UI:
  - A merchant user only ever sees records whose `merchant_id` belongs to a
    merchant they own. An admin token sees **all** merchants' records — the
    dashboard pages must therefore always render a `merchant_id` column/value
    (admins need it; merchants need it too, see gap D1/S8: disputes cannot be
    filtered by merchant server-side).
  - The active merchant is resolved exactly as `transactions.astro` does:
    `localStorage["mp_merchant"]` (plain id or JSON `{id|merchant_id}`),
    falling back to the first entry of `GET /api/v1/merchants`
    (`listMerchants()` in `lib/api.ts`), persisted back to `mp_merchant`.
  - No token → redirect to `/login` (copy the `init()` guard in
    `transactions.astro` verbatim in behaviour).

### 1.2 Error envelope (all endpoints, `dto.rs` Wave 45)

```json
{ "error": "<human message>", "code": "<stable code>", "status": <http status> }
```

Codes: `bad_request` (400), `unauthorized` (401), `forbidden` (403),
`not_found` (404), `conflict` (409), `internal_error` (500).
`lib/api.ts::apiFetch` already throws `Error(body.error)` — pages surface that
message in the page-level `<p class="alert" role="status">` exactly like
`transactions.astro` (`showMessage(msg, "error")`), and use `toast()` /
inline field errors for form-level failures.

### 1.3 Money shape (nested — do not flatten)

`Money` serialises as a nested object everywhere in this stream:

```json
{ "amount_minor": 50000, "currency": "BDT" }
```

- `currency` is the bare enum variant string: one of `BDT | INR | PKR | USD | EUR`
  (no object, no `code` field — `transactions.astro`'s tolerant `currencyCode()`
  helper also handles this; reuse that helper pattern).
- All currencies have 100 minor units per major unit. Display with the existing
  `formatMoney(amount_minor, currency)` from `lib/api.ts`
  (e.g. `formatMoney(50000, "BDT")` → `"BDT 500.00"`).
- There is **no flat `amount_minor`/`currency` pair** on `Dispute` or
  `Settlement` top level. Dispute amount is `dispute.amount.amount_minor`;
  settlement amounts are `settlement.gross.amount_minor` and
  `settlement.fee.amount_minor`.

### 1.4 Timestamps

All timestamps are RFC 3339 UTC (`DateTime<Utc>`), e.g. `opened_at`,
`decided_at`, `period_start`, `period_end`, `created_at`, `added_at`.
Render with the `formatDate()` pattern from `transactions.astro`
(`new Date(v).toLocaleString()`, `—` when absent/unparseable).

### 1.5 Pagination primitives (`pagination.rs`)

- Defaults: `page = 1`, `per_page = 20`, `per_page` clamped to `1..=100`.
- ⚠️ `apply_page` quirk, both list endpoints: **if neither `page` nor
  `per_page` is supplied, the full unpaginated list is returned.** Both pages
  in this stream must therefore **always** send explicit `page` and
  `per_page` parameters. Use `per_page = 25` to match the transactions page's
  `PAGE_SIZE = 25` client convention.

### 1.6 Page skeleton (copy `transactions.astro`)

Both pages use, in order: `BaseLayout` → `DashboardNav` →
`<main id="main" class="container stack">` → `PageHeader` →
`<p id="…-message" class="alert" role="status" hidden>` →
`#no-merchant` card (hidden) → `#…-panel` (hidden until merchant resolved) →
filter form card → list card containing `#…-loading` paragraph,
`#…-empty` wrapping `EmptyState`, `.table-wrap > table.table` (hidden until
rows exist), and a `.pagination` row (`← Previous` / `Page N` / `Next →`,
`btn btn-ghost btn-sm`). Client script is an inline `<script>` importing from
`../../lib/api` and `../../lib/ui`. DOM must be built with `textContent`
(createElement pattern from `transactions.astro`), never `innerHTML` with
server strings — the only sanctioned `innerHTML` is the status cell receiving
`statusBadge(status)` output, as in `transactions.astro`.

### 1.7 `lib/api.ts` additions needed (C3/C4, additive only)

No dispute/settlement helpers exist in `lib/api.ts` today. Add (names are the
contract for C3/C4):

```ts
export interface MoneyView { amount_minor: number; currency: string; }

export interface EvidenceFileView { name: string; url: string; added_at: string; }

export interface Dispute {
  id: string; merchant_id: string; transaction_id: string;
  amount: MoneyView;
  reason: 'fraudulent' | 'product_not_received' | 'duplicate' | 'credit_not_processed' | 'other';
  status: 'opened' | 'under_review' | 'won' | 'lost' | 'closed';
  evidence_note: string | null;
  evidence_files: EvidenceFileView[];
  outcome_note: string | null;
  opened_at: string; decided_at: string | null;
}

export interface Settlement {
  id: string; merchant_id: string;
  gross: MoneyView; fee: MoneyView;
  status: 'pending' | 'processing' | 'paid' | 'failed' | 'on_hold';
  period_start: string; period_end: string;
  payout_method: string; payout_reference: string | null;
  created_at: string;
}
```

Call the endpoints with `apiFetch` directly (as `transactions.astro` does for
refunds) or add thin wrappers; either is acceptable, but the interfaces above
must be used for rendering so the two pages agree on shapes.

---

## 2. Endpoint inventory — Disputes

Base: all under `/api/v1`. Handler: `handlers/disputes.rs`. Domain: `dispute.rs`.

### 2.1 `POST /api/v1/disputes` — open a dispute — `201 Created`

Auth: merchant owner of the transaction's merchant, or admin.

Request body (`OpenDisputeRequest`):

```json
{ "transaction_id": "tx_…", "amount_minor": 50000, "currency": "BDT", "reason": "fraudulent" }
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `transaction_id` | string | yes | Must exist; non-owner's tx → `404` (existence not leaked) |
| `amount_minor` | i64 | yes | Minor units; `> 0` and `≤` transaction amount |
| `currency` | string \| absent | no | ISO code; defaults to the transaction's currency; must match it if given |
| `reason` | enum | yes | See §2.5 |

Validation (`validate_open`, all errors joined with `"; "` into one `400`):
transaction status must be `completed`; currency must match the transaction;
amount positive; amount must not exceed the transaction amount.
Duplicate guard: a transaction that already has a **non-terminal** dispute →
`409 Conflict` `"transaction … already has an open dispute"`.
Side effects: audit `dispute.opened`, webhook `dispute.opened`.
Response: the full `Dispute` object (§2.6).

Production note for copy: in production a dispute is normally opened by the
provider's chargeback notice; this endpoint is the platform's manual record.
UI copy must not imply the merchant "starts a chargeback against themselves" —
see §5.4 for the (secondary) open form's wording.

### 2.2 `GET /api/v1/disputes` — list — `200`

Auth: any authenticated user; results are visibility-filtered server-side.

Query parameters: **only** `page`, `per_page` (`PageQuery`). There are **no**
`status`, `reason`, `merchant_id`, `transaction_id` or date filters (gap D1).

Response envelope (note: object, not a bare array):

```json
{ "items": [ /* Dispute, newest first by opened_at */ ], "total": 42 }
```

- `total` is the count of **all disputes visible to the caller** (computed
  before paging), so total pages = `ceil(total / per_page)`. This is the one
  list in this stream that gives the UI a real total — use it for the pager
  (`Next` disabled when `page * per_page >= total`), unlike the transactions
  page's row-count heuristic.
- Sort: `opened_at` descending, server-side. No sort parameter.

### 2.3 `GET /api/v1/disputes/:id` — detail — `200`

Returns the full `Dispute`. Invisible/nonexistent → uniform `404`
(`"dispute {id} not found"`). This is the fetch the detail view (§5.3) uses,
so deep-linking/refresh of a selected dispute works.

### 2.4 `POST /api/v1/disputes/:id/evidence` — add/replace evidence note — `200`

Request body (`EvidenceRequest`): `{ "note": "…" }`.

- `note`: 1–2000 **characters** after trim check (empty/whitespace-only →
  `400 "evidence note must be 1..=2000 characters"`).
- ⚠️ Semantics: `evidence_note` is a **single field that is overwritten** by
  each call. It is not an append-only log; there is no author or timestamp on
  the note. The UI must warn about replacement (§5.5).
- State transition: `opened → under_review` on first evidence (note or file);
  already `under_review` stays. Terminal dispute (`won|lost|closed`) →
  `409 "dispute {id} is already decided"`.
- Response: the updated full `Dispute`.

### 2.5 `POST /api/v1/disputes/:id/evidence/files` — attach evidence file reference — `200`

Request body (`EvidenceFileRequest`): `{ "name": "…", "url": "https://…" }`.

- ⚠️ **There is no file upload.** No multipart endpoint exists anywhere in the
  route table; the platform stores a **reference only, never bytes**. The UI
  must **not** render `<input type="file">` for evidence. The form is two text
  inputs: display `name` + hosted-file `url` (§5.5).
- `name`: 1–200 chars → else `400 "evidence file name must be 1..=200 characters"`.
- `url`: must start with `https://` and be ≤ 2000 chars → else
  `400 "evidence file url must be an https URL of at most 2000 characters"`.
  No content-type allowlist, no size limit — because no bytes are transferred.
- Files **append** to `evidence_files[]` (unlike the note). No delete/remove
  endpoint exists (gap D4). Same `opened → under_review` transition and same
  `409` on terminal disputes as §2.4.
- Response: the updated full `Dispute`.

### 2.6 `POST /api/v1/admin/disputes/:id/decide` — admin only — `200`

Request body (`DecideRequest`): `{ "outcome": "won" | "lost", "note": "…" }`
(`note` optional). Non-admin → `403 "admin only"`. Other `outcome` values →
`400`. Already terminal → `409`. Sets `status`, `outcome_note`,
`decided_at = now`; emits webhook `dispute.closed` + audit `dispute.decided`.
**The merchant pages in this stream must not render decide controls.** (An
admin operating the merchant dashboard still uses the admin surface for this;
out of Stream C scope.)

### 2.7 `Dispute` response shape (exact)

```json
{
  "id": "dsp_…",
  "merchant_id": "mch_…",
  "transaction_id": "tx_…",
  "amount": { "amount_minor": 50000, "currency": "BDT" },
  "reason": "product_not_received",
  "status": "under_review",
  "evidence_note": "…" | null,
  "evidence_files": [ { "name": "receipt.pdf", "url": "https://…", "added_at": "2026-…" } ],
  "outcome_note": "…" | null,
  "opened_at": "2026-…",
  "decided_at": "2026-…" | null
}
```

### 2.8 Dispute enums (exact wire values)

`DisputeReason` (`#[serde(rename_all = "snake_case")]`):
`fraudulent`, `product_not_received`, `duplicate`, `credit_not_processed`, `other`.
UI labels: Fraudulent / Product not received / Duplicate / Credit not processed / Other.

`DisputeStatus` (snake_case): `opened`, `under_review`, `won`, `lost`, `closed`.
Terminal = `won | lost | closed` (`is_terminal()`); evidence and decisions are
rejected once terminal. Note: **no code path currently sets `closed`** — decide
only produces `won`/`lost`. The UI must still badge `closed` correctly if it
ever appears, but must not offer a "close" action.

Fields that do **not** exist on a dispute (do not render, do not fabricate):
no customer name/email/phone, no gateway, no provider chargeback id, no
respond-by/due date, no transaction date/amount-of-transaction, no evidence
history. See gap D5/D6 for the enrichment workaround.

---

## 3. Endpoint inventory — Settlements (Payouts)

Handler: `handlers/settlements.rs`. Domain: `settlement.rs`. Job logic:
`worker/settlement_job.rs`.

### 3.1 `GET /api/v1/settlements` — list — `200`

Query (`ListSettlementsQuery`): `merchant_id?`, `status?`, `page?`, `per_page?`.

Response: ⚠️ a **bare JSON array** of `Settlement` — no envelope, no `total`
(gap S1):

```json
[ /* Settlement, sorted by created_at descending */ ]
```

- Visibility: owner-scoped server-side (admins: all), applied **after** the
  `merchant_id`/`status` field filters. Passing another user's `merchant_id`
  yields `[]`, not an error.
- Sort: `created_at` descending ("newest period first" per handler doc; the
  sort key is `created_at`, and settlements from one run share a timestamp —
  treat intra-run order as unspecified).
- ⚠️ Status filter matching quirk (gap S3): the handler compares
  `format!("{:?}", status)` (the Rust Debug name: `Pending`, `Processing`,
  `Paid`, `Failed`, `OnHold`) case-insensitively against the query value.
  Therefore `status=pending`, `paid`, `failed`, `processing` work, but the
  natural snake_case `status=on_hold` **never matches** (`OnHold` ≠ `on_hold`);
  only `onhold`/`OnHold` matches. Unknown values silently return `[]`.
  Until C2 fixes S3, the C4 page must send exactly these filter values:
  `Pending`, `Processing`, `Paid`, `Failed`, `OnHold` (capitalised Debug
  names) — or omit `status` for "all".

### 3.2 `POST /api/v1/settlements/run` — build settlements for a period — `201 Created`

Request body (`RunSettlementsReq`):

```json
{ "merchant_id": "mch_…", "period_start": "2026-11-01T00:00:00Z", "period_end": "2026-11-08T00:00:00Z", "fee_bps": 250 }
```

| Field | Type | Required | Notes |
|---|---|---|---|
| `merchant_id` | string | yes | Caller must own it or be admin, else `403 "forbidden"` |
| `period_start` / `period_end` | RFC 3339 `DateTime<Utc>` | yes | Half-open `[start, end)`; `period_end <= period_start` → `400` |
| `fee_bps` | u32 | no | Defaults to platform `default_fee_bps` (250 = 2.5% in the seeded `PlatformSettings`) when omitted. ⚠️ See gap S4: a merchant caller can currently pass `0` and waive their own fee — **the UI must never send `fee_bps`** |

What a run does (`collect_candidates` + `build_settlements`):

- Considers only the merchant's transactions with status `completed` whose
  `created_at` ∈ `[period_start, period_end)`.
- Excludes any transaction whose `created_at` falls inside the period of an
  earlier settlement for the same merchant with status ≠ `failed`
  (double-settlement guard — re-running an overlapping period does not pay
  twice; it typically creates fewer or zero settlements).
- Groups remaining amounts by currency, drops non-positive groups, and creates
  **one `Settlement` per currency** with `status: "pending"`,
  `payout_method: "manual"`, `payout_reference: null`,
  `fee = round-half-up(gross × fee_bps / 10000)`.
- A run with no candidates is **not** an error: `201` with `created: 0`.

Response:

```json
{ "created": 1, "settlements": [ /* Settlement[] */ ] }
```

Side effects: audit `settlement.run`. No webhook, **no money moves** — a run
only computes records.

### 3.3 `POST /api/v1/settlements/:id/pay` — record manual payout — `200`

Request body (`PaySettlementReq`): `{ "payout_reference": "bank_trx_…" }`
(non-empty after trim, else `400`).

- Transition (`mark_paid`): only from `pending` or `processing` → `paid`,
  storing `payout_reference`. From `paid | failed | on_hold` → `409` with the
  domain message (e.g. `"settlement … cannot be marked paid from status Paid"`).
- Auth in code: **owner or admin** (`may_access`), checked after the record is
  loaded — nonexistent id → `404`; existing but not yours → `403 "forbidden"`
  (note the inconsistency with disputes' uniform `404`; gap S6).
- Side effects: audit `settlement.paid`, webhook `settlement.paid` with
  `gross_minor`, `fee_minor`, `net_minor`, `currency`, `payout_reference`.
- ⚠️ Product semantics (decision, §6.4): "pay" records that **the platform**
  sent money **to** the merchant, off-platform (bank/wallet transfer), and the
  reference is the rail's transaction id. A merchant marking their *own*
  settlement paid is self-attestation of receiving money nobody verified, and
  it corrupts the balance ledger: `compute_balance` subtracts the **net of
  `paid` settlements** from `available_minor`. The merchant UI therefore does
  **not** expose Pay (see §6.4 and gap S5 for the C2 hardening).

### 3.4 There is **no** `GET /api/v1/settlements/:id`

No detail endpoint exists (gap S2). All fields a detail view needs are already
in the list payload, so the Payouts page detail is an **expanded row built
from list data** (§6.3), never a fetch, and there is no
`/dashboard/payouts/[id]` route in this stream.

### 3.5 `Settlement` response shape (exact)

```json
{
  "id": "stl_…",
  "merchant_id": "mch_…",
  "gross": { "amount_minor": 100000, "currency": "BDT" },
  "fee":   { "amount_minor": 2500,   "currency": "BDT" },
  "status": "pending",
  "period_start": "2026-11-01T00:00:00Z",
  "period_end":   "2026-11-08T00:00:00Z",
  "payout_method": "manual",
  "payout_reference": null,
  "created_at": "2026-…"
}
```

- **Net is not a field.** It is derived: `net_minor = gross.amount_minor − fee.amount_minor`,
  in `gross.currency` (server guarantees `fee.currency == gross.currency` for
  records built by `build_settlements`; if a row ever shows differing
  currencies, render net as `—` rather than a wrong number). UI label: "Net payout".
- `payout_method` is `"manual"` for every settlement the current job builds.
  Do not render it as "bKash"/"bank" — the honest label is "Manual payout".
- There is **no payout destination** (no bank account / wallet number) on the
  record or in merchant settings (gap S7). Never imply where money was sent
  beyond `payout_method` + `payout_reference`.

### 3.6 Settlement status enum (exact wire values)

`pending`, `processing`, `paid`, `failed`, `on_hold` (snake_case serialisation).
Terminal = `paid | failed`. Meanings for UI copy (from domain docs):

| Status | Merchant-facing explanation |
|---|---|
| `pending` | Calculated for the period; payout not yet sent. |
| `processing` | Payout has been handed to the payout provider/bank. (No endpoint in this stream sets it — it can only arrive from worker/other code; badge it, offer no action.) |
| `paid` | Payout sent; `payout_reference` is the transfer reference. |
| `failed` | Payout failed and needs platform attention before retry. |
| `on_hold` | Held back, e.g. for compliance or dispute review. |

### 3.7 Where the "schedule" actually lives

Merchant payout *preferences* exist in merchant settings, **not** on settlements:

`GET /api/v1/merchants/:id/settings` → `MerchantSettings` (already typed in
`lib/api.ts`): `payout_schedule: "manual" | "daily" | "weekly" | "monthly" | null`
(null/absent = manual), `payout_day: number | null` (weekly: 1=Mon..7=Sun;
monthly: 1..28), `auto_settle: boolean`, `settle_threshold_minor: number`,
`fee_bps: number | null` (merchant override; null = platform default).
These are already editable on `/dashboard/settings` (Payouts section).

⚠️ **Honesty requirement:** no code consumes `payout_schedule` / `auto_settle`
to create settlements automatically — settlements come into existence **only**
via `POST /settlements/run`. The Payouts page must present the schedule as a
*configured preference* with a link to Settings, and must state that payouts
are currently calculated/run manually. It must not promise "your next payout
arrives Friday" or compute a next-payout date from the schedule (gap S8).

Related read the Payouts header may use: `GET /api/v1/merchants/:id/balance`
(`merchantBalance()` in `lib/api.ts`) → `{ merchant_id, available_minor,
pending_minor, currency }` where `available = completed − succeeded refunds −
net of paid settlements`, computed **in BDT only** (handler hardcodes
`Currency::BDT`). Label it exactly "Available balance (BDT)".

---

## 4. API gap list (for C2) — minimal fixes & workarounds

Severity: **B** = blocks a correct UI, **S** = security/correctness, **P** = polish.
"Workaround" is what C3/C4 do if C2's fix does not land; fixes are deliberately
minimal and additive.

### Disputes

- **D1 (B) — `GET /disputes` has no filters.** Only `page`/`per_page`; a
  multi-merchant user (and every admin) gets all visible merchants' disputes
  mixed, and status filtering is impossible server-side.
  *Minimal fix (C2):* extend the list handler's query struct (replace bare
  `PageQuery`) with optional `status`, `reason`, `merchant_id`,
  `transaction_id`; filter the `visible` vec on exact (case-insensitive for
  `status`/`reason`, parsed against the enums) matches **before** computing
  `total` and paging. Visibility filtering stays as-is.
  *Workaround:* C3 always sends `?page=N&per_page=100`, client-filters by
  status/reason/active-merchant over the fetched set, and labels the pager
  honestly ("Showing filtered results from the first 100 disputes") — plus
  always renders the `merchant_id` column. If D1 lands, C3 switches to
  server-side `status` + `merchant_id` params and `per_page=25` paging using
  `total`. **C3: implement against the fixed contract, feature-detect by
  always sending the params (unknown params are ignored by serde query
  extraction today, so this is safe pre-fix) — but the client-side filter must
  remain as a no-op-safe pass when rows already match.**

- **D2 (P) — Note is a single overwritten field, no history.** `evidence_note`
  has no author/timestamp/revision list.
  *Fix:* none in v0.0.4 (a real fix is a domain change to an evidence-entry
  list — explicitly out of scope).
  *Workaround:* UI pre-fills the form with the current note and warns
  "Saving replaces your previous note" (§5.5).

- **D3 (P) — No evidence-file deletion.** Files append forever until decision.
  *Fix (optional, C2 only if time):* none specified. *Workaround:* UI copy
  "Files can't be removed once attached — check the link before adding."

- **D4 (B, expectation) — No file upload bytes.** Only `https://` URL
  references (§2.5); there is no storage/multipart/presign endpoint.
  *Fix:* none in v0.0.4. *Workaround:* the name+URL form in §5.5 is the spec;
  C3 must not build a file picker.

- **D5 (P) — Dispute carries no transaction context.** No customer, gateway,
  transaction date or original total beyond `transaction_id`.
  *Fix:* none required. *Workaround:* detail view (§5.3) makes one extra call,
  `GET /api/v1/transactions/:id` (owner-scoped list/detail already used by the
  dashboard), and renders gateway/customer/created_at/status from it; on any
  error it silently omits the enrichment block (the dispute itself is the
  source of truth). List rows do **not** enrich (would be N+1).

- **D6 (P) — No deadline / provider reference.** No respond-by date or
  chargeback id exists, so no countdown/SLA UI is possible.
  *Fix:* none. *Workaround:* show `opened_at` and status only; never fabricate
  a deadline.

### Settlements / Payouts

- **S1 (B) — List returns a bare array, no `total`.** The UI cannot render a
  true "Page N of M".
  *Minimal fix (C2):* return `{ "items": [...], "total": <visible count
  before paging> }`, mirroring `GET /disputes` exactly.
  *Workaround:* C4 uses the transactions-page heuristic: `Next` enabled iff
  the returned row count == `per_page` (25); label is just "Page N". C4 should
  accept both shapes (`Array.isArray(result) ? result : result.items`) like
  `transactions.astro` tolerates envelopes.

- **S2 (B) — No `GET /settlements/:id`.**
  *Minimal fix (C2):* add `get_settlement` mirroring disputes'
  `load_visible` (uniform `404` for invisible/nonexistent) and register
  `.route("/api/v1/settlements/:id", get(...))` — ⚠️ register it **after**
  `/api/v1/settlements/run` in the builder chain is not required in Axum 0.7
  (distinct method+segment counts), but C2 must add a regression test that
  `POST /settlements/run` still routes.
  *Workaround:* detail = expanded row from list data (§6.3); no separate
  route. If S2 lands, C4 still uses the expanded row (list payload is
  complete); the endpoint is for future deep-linking.

- **S3 (B) — Status filter doesn't accept the wire format.** Filter compares
  Debug names case-insensitively, so `?status=on_hold` (the actual serialised
  value) returns `[]` (§3.1).
  *Minimal fix (C2):* parse the query value into `SettlementStatus` via its
  serde representation (accept snake_case; ideally also accept the legacy
  Debug spelling) and compare enum-to-enum; unknown value → `400`, not `[]`.
  *Workaround:* C4 sends capitalised Debug spellings (`OnHold` etc.) until
  the fix lands, per §3.1.

- **S4 (S) — `fee_bps` is merchant-settable on `/run`.** Any owner can pass
  `fee_bps: 0` and create fee-free settlements for themselves, bypassing the
  platform fee (default 250 bps) and their merchant `fee_bps` setting.
  *Minimal fix (C2):* for non-admin callers, ignore/reject a client-supplied
  `fee_bps` — resolve it as
  `merchant_settings.effective_fee_bps(platform_settings)` (merchant override
  else platform default); only admins may pass an explicit `fee_bps`.
  Rejecting with `400` is acceptable; silently ignoring is not (auditability).
  *UI side:* C4's run form never sends `fee_bps` (§6.5) — but note in the PR
  description that hiding the field is not the fix; S4 is.

- **S5 (S) — `/settlements/:id/pay` is owner-callable.** `may_access` lets a
  merchant mark their own settlement `paid` with any reference string, which
  (a) falsely asserts the platform sent money, and (b) reduces their computed
  `available_minor` (paid nets are subtracted in `compute_balance`), corrupting
  the ledger the Overview balance shows.
  *Minimal fix (C2):* in `pay_settlement`, replace the `may_access` check with
  an admin check mirroring `decide_dispute`: `if !auth.is_admin() { 403
  "admin only" }`. Keep `mark_paid` transition rules unchanged.
  *UI side:* merchant Payouts page renders **no Pay control at all** (§6.4),
  regardless of whether S5 has landed.

- **S6 (P) — Inconsistent existence leakage in settlements.** `pay` returns
  `404` for unknown id but `403` for someone else's existing id; disputes use
  uniform `404`. *Fix (C2, with S5):* after the admin gate this matters less;
  optionally mirror disputes' load-then-404 pattern. No UI impact.

- **S7 (P) — No payout destination on record.** `payout_method` is always
  `"manual"`; no bank/wallet account is stored on the settlement or in
  settings, so the UI cannot say where a payout went.
  *Fix:* none in v0.0.4 (needs a payout-account settings field + job change).
  *Workaround:* render method as "Manual payout" + reference verbatim (§6.3).

- **S8 (P, honesty) — Schedule/auto-settle settings are inert.** Nothing runs
  settlements on the configured `payout_schedule`/`auto_settle` (§3.7).
  *Fix:* none in Stream C (a worker scheduler is a separate wave).
  *Workaround:* §6.2's schedule card is descriptive + links to Settings; the
  only creation action is the explicit Run form (§6.5).

### Shared / frontend

- **F1 (B) — `statusBadge` has no entries for this stream's statuses.**
  `lib/ui.ts` BADGES contains `open` but not `opened`, and lacks
  `under_review`, `won`, `lost`, `closed`, `on_hold`. The fallback would
  render uncapitalised text ("under review") on the default badge.
  *Fix (C3, in `lib/ui.ts`, additive):*
  `opened: { cls: "badge-pending", label: "Opened" }`,
  `under_review: { cls: "badge-pending", label: "Under review" }`,
  `won: { cls: "badge-success", label: "Won" }`,
  `lost: { cls: "badge-failed", label: "Lost" }`,
  `closed: { cls: "badge", label: "Closed" }`,
  `on_hold: { cls: "badge-pending", label: "On hold" }`.
  (`pending`, `processing`, `paid`, `failed` already exist with correct
  classes.) C4 depends on `on_hold`; C3 lands F1 for both pages.

- **F2 (P) — No nav entries / i18n keys.** See §7.

---

## 5. Page spec — `/dashboard/disputes` (C3)

File: `frontend/src/pages/dashboard/disputes.astro`. Nav key: `disputes`.

### 5.1 Header

`PageHeader title="Disputes"`, sub:
"Chargebacks and payment challenges for your merchants — add evidence before
the platform decides." Plus the standard `#disputes-message` alert and
`#no-merchant` card (text adapted: disputes live under a merchant).

Note the deliberate wording: the list is **visibility-scoped, not
active-merchant-scoped** (the API has no merchant filter pre-D1). If D1 has
landed, C3 passes `merchant_id=<active merchant>` and the sub becomes
"…for the active merchant."; pre-D1 the page shows all owned merchants and the
`Merchant` column (§5.2) is mandatory. C3 implements the D1 workaround path
(§4-D1) and keeps the column either way.

### 5.2 List section

Filter form (`#disputes-filters`, same layout classes as `#tx-filters`):

| Control | id | Values | Behaviour |
|---|---|---|---|
| Status select | `#filter-status` | `all`, `opened`, `under_review`, `won`, `lost`, `closed` | Sent as `status=` if D1 landed; client-filter otherwise |
| Reason select | `#filter-reason` | `all` + the 5 reasons (§2.8) with UI labels | Same as status |
| Reset button | `#filter-reset` | — | Back to `all`/`all`, page 1, reload |

No free-text search (no API support; do not fake one).

Table (`#disputes-table`), columns in order:

| Column | Source | Rendering |
|---|---|---|
| Opened | `opened_at` | `formatDate()` |
| ID | `id` | `mono`, truncated via `shortId()` pattern, full id in `title` |
| Merchant | `merchant_id` | `mono`, truncated (see §5.1) |
| Transaction | `transaction_id` | `mono`, truncated; plain text (no tx detail page exists to link to) |
| Reason | `reason` | UI label from §2.8 map |
| Amount | `amount.amount_minor`, `amount.currency` | `formatMoney(...)`, cell class `num` |
| Status | `status` | `statusBadge(status)` (§4-F1) |
| Evidence | derived | `📎 n` equivalent in text: `"1 note · 2 files"` / `"—"` — count `evidence_note ? 1 : 0` notes and `evidence_files.length` files; text only, no icon font assumption |
| Actions | — | `View` button (`btn btn-ghost btn-sm`) toggling the detail row (§5.3); label becomes `Hide` while open |

Pager: `#disputes-pagination` with `#disputes-prev`, `#disputes-page-label`,
`#disputes-next`. With the disputes envelope, `total` is real:
label `"Page N of M"` (M = `ceil(total/per_page)`, min 1 when total > 0),
`Prev` disabled on page 1, `Next` disabled when `page >= M`. Page size 25
(pre-D1 workaround mode: per §4-D1 the fetch uses `per_page=100` and the
pager operates over the client-filtered array in steps of 25 — implement one
code path: fetch envelope → full visible array for the current server page →
client-filter → slice for display; keep the two modes behind a comment).

States:
- **Loading:** `#disputes-loading` paragraph "Loading disputes…" shown during
  fetch (pattern from transactions); `skeletonAfter()` from `lib/ui.ts` is
  acceptable but the paragraph alone matches the model page — use the paragraph.
- **Empty (no disputes at all):** `EmptyState` title
  "No disputes yet." body: "When a payer challenges a payment, the dispute
  will appear here so you can respond with evidence."
- **Empty (filters):** `EmptyState` title "No disputes match these filters."
- **Error:** table hidden, `#disputes-message` alert-error with the API
  `error` message; a `Retry` button in the empty/error area re-calls load
  (the model page has no retry button; adding one ghost button is sanctioned
  here because list failure otherwise strands the page).

### 5.3 Detail view (expanded row, not a separate route)

Clicking `View` inserts one detail row (`<tr>` with a single
`<td colSpan=9>`) under the dispute row — the same expandable-row technique as
the transactions refund panel (`toggleRefundPanel`). Only one detail row open
at a time; opening another closes the first. On open, re-fetch fresh data with
`GET /api/v1/disputes/:id` (the list row may be stale after evidence posts);
on fetch error, render the detail from the list row and show the error inline
in the detail row.

Detail contents, in order:
1. **Summary grid** (definition list): Dispute ID (full, `mono`), Merchant ID,
   Transaction ID, Amount (`formatMoney`), Reason (label), Status (badge),
   Opened (`formatDate(opened_at)`), Decided (`formatDate(decided_at)` or `—`).
2. **Transaction context** (only if the D5 enrichment fetch
   `GET /api/v1/transactions/:transaction_id` succeeds): Gateway, transaction
   status (badge), transaction amount, customer email/phone, transaction date.
   Silently omitted on failure.
3. **Outcome block** — only when terminal: "Decision: Won/Lost" + 
   `outcome_note` (or "No outcome note recorded.") + decided date. Copy must
   state the domain honesty note: "A decided dispute does not move money by
   itself; any clawback is handled through settlements."
4. **Evidence block** (§5.5): current note, file list, and — when
   non-terminal — the two add forms. When terminal, forms are replaced by the
   text "This dispute has been decided; evidence can no longer be added."

### 5.4 Open-dispute form (secondary, collapsed)

A `<details>`-style collapsed card under the list, heading "Record a dispute
manually", with copy: "Disputes are normally opened by the payer's bank or
wallet provider. Use this only to record one you've been notified about."
Fields: Transaction ID (text, required), Amount (major units, number,
step 0.01 — convert with `Math.round(major * 100)`, prefill nothing),
Currency (select of `BDT/INR/PKR/USD/EUR`, default `BDT`), Reason (select,
§2.8). Submit → `POST /api/v1/disputes`; on `201`, success alert
"Dispute <shortId> recorded." and reload page 1; on error, inline error text
in the form (the joined validation message from §2.1 is genuinely useful
here). This form is explicitly secondary: no nav entry, no header button.

### 5.5 Evidence UI (inside detail row)

**Current evidence:**
- Note: if `evidence_note` non-null, render it in a bordered block labelled
  "Evidence note (latest)"; else "No evidence note yet."
- Files: list of `evidence_files[]`, each row: `<a href={url} target="_blank"
  rel="noopener noreferrer">{name}</a>` + `added_at` formatted + the URL in
  `mono small` text. Build anchors with `createElement` and set `href` from
  the value (the API guarantees `https://` prefix on write, but render-time
  code must still only set the href if it starts with `https://`, else render
  the URL as plain text — defence in depth against stored data).

**Add note form** (non-terminal only): textarea `#evidence-note-<id>`,
`maxlength` not set in HTML (server counts characters; set `maxlength="2000"`
anyway as a hint), prefilled with the current `evidence_note ?? ""`.
Warning line above it, verbatim intent: "Saving replaces your previous note —
there is no history." Submit button "Save evidence note" →
`POST /api/v1/disputes/:id/evidence` `{ "note": <value> }`. On success:
re-render detail from the response `Dispute` (status may have flipped to
`under_review` — the row badge must update too; simplest correct approach:
reload the list page state after a successful evidence post, keeping the same
dispute's detail open). On `409`: show "This dispute has already been
decided." and re-fetch. Empty note → client-side field error via
`fieldError()` from `lib/ui.ts` ("Evidence note must not be empty.") without
a network call.

**Add file form** (non-terminal only): two text inputs — Name
(`maxlength="200"`, placeholder "e.g. Delivery receipt (PDF)") and File URL
(`type="url"`, placeholder "https://…"). Helper text, verbatim intent:
"Link to a file hosted online (receipt, screenshot, delivery proof). Files
are linked, not uploaded, and can't be removed once attached." Submit
"Attach file link" → `POST /api/v1/disputes/:id/evidence/files`
`{ "name", "url" }`. Client-side pre-checks mirror the server (name 1–200
chars; URL starts with `https://`, ≤ 2000 chars) using `fieldError()`;
server errors surface inline. On success, re-render detail from the response.

**Respond flow summary** (what "responding to a dispute" means in this UI):
open detail → add note and/or file links → status visibly moves
`Opened → Under review` → merchant waits; decision is admin-side only. The
detail row must say exactly that in one line when non-terminal:
"Adding evidence moves this dispute to Under review. A platform admin records
the final decision."

---

## 6. Page spec — `/dashboard/payouts` (C4)

File: `frontend/src/pages/dashboard/payouts.astro`. Nav key: `payouts`.
Data: settlements of the **active merchant** — unlike disputes, this list
*does* support `merchant_id`, and C4 always sends
`merchant_id=<active merchant>` (admins browsing the merchant dashboard also
get the active-merchant view; the all-merchants view is an admin-surface
concern, out of scope).

### 6.1 Header

`PageHeader title="Payouts"`, sub: "Settlements for the active merchant — what
was collected each period, the platform fee, and what was paid out."
Standard `#payouts-message` alert + `#no-merchant` card.

### 6.2 Summary & schedule card (above the list)

A card row (use `StatCard`-style markup consistent with the dashboard index)
fed by two calls on init:
1. `GET /api/v1/merchants/:id/balance` → StatCard "Available balance" =
   `formatMoney(available_minor, currency)` (currency comes from the response;
   backend computes in BDT, §3.7), hint "Completed payments minus refunds and
   paid-out settlements". Second card "Pending payments" =
   `formatMoney(pending_minor, currency)`, hint "Not yet captured — not yet
   payable". If the balance call fails, omit both cards silently (list is the
   primary content).
2. `GET /api/v1/merchants/:id/settings` (`getMerchantSettings()`) → schedule
   card, text built from `payout_schedule`/`payout_day`:
   - `manual` or null → "Payout schedule: Manual — settlements are created
     when a settlement run is requested."
   - `daily` → "Payout schedule: Daily (preference)."
   - `weekly` → "Payout schedule: Weekly, day {payout_day} (1 = Monday …
     7 = Sunday) (preference)."
   - `monthly` → "Payout schedule: Monthly, day {payout_day} (preference)."
   Followed by the honesty line (mandatory, §3.7/S8): "Schedules are a saved
   preference — payouts are currently run manually, not automatically." and a
   link "Edit payout settings" → `/dashboard/settings`.
   If the settings call fails, omit the card silently.

Do **not** compute "next payout", "expected payout date", or pending-payout
totals by summing the current page — a page is a window, not the ledger. (A
"Pending settlements on this page" subtotal is also forbidden: it invites
misreading. Totals come from the balance endpoint only.)

### 6.3 List section

Filter form (`#payouts-filters`): one Status select `#filter-status` with
`all`, `pending`, `processing`, `paid`, `failed`, `on_hold` (labels from
§3.6) + Filter/Reset buttons as per the model page. The **query value sent**
follows §3.1/S3: pre-fix send `Pending|Processing|Paid|Failed|OnHold`; if C2
lands S3 first, send the snake_case wire values. C4: implement a named
constant map `STATUS_FILTER_QUERY: Record<string,string>` with a comment
citing gap S3, so flipping it post-fix is a one-line change.

Table (`#payouts-table`), columns:

| Column | Source | Rendering |
|---|---|---|
| Period | `period_start` → `period_end` | `"{date(start)} – {date(end)}"` using date-only formatting (`toLocaleDateString()`); full timestamps in the detail row. Note in a table footnote: "Periods are [start, end): a payment exactly at the end belongs to the next period." |
| ID | `id` | `mono`, truncated, full in `title` |
| Gross | `gross` | `formatMoney(gross.amount_minor, gross.currency)`, `num` |
| Fee | `fee` | `formatMoney(fee.amount_minor, fee.currency)`, `num` |
| Net payout | derived | `formatMoney(gross.amount_minor − fee.amount_minor, gross.currency)`, `num`, semibold; `—` if `fee.currency !== gross.currency` (§3.5) |
| Method | `payout_method` | `"manual"` → "Manual payout"; any other value rendered verbatim |
| Reference | `payout_reference` | `mono`, or `—` when null |
| Status | `status` | `statusBadge(status)` (needs F1 `on_hold`) |
| Actions | — | `Details` toggle button only (see §6.4 for what is *not* here) |

Pager: bare-array contract (S1 workaround): label "Page N", `Prev` disabled
on page 1, `Next` disabled when returned rows < 25. If C2 lands S1, accept
`result.items` + `total` and upgrade the label to "Page N of M" — write the
row-extraction as `Array.isArray(result) ? result : result.items ?? []` from
day one.

States:
- **Loading:** "Loading payouts…" paragraph, as per model page.
- **Empty (none at all):** `EmptyState` title "No payouts yet." body:
  "When a settlement run covers a period with completed payments, the payout
  for that period will appear here." + a button that scrolls to the Run form
  (§6.5) — use `scrollToEl()` from `lib/ui.ts`.
- **Empty (filter):** `EmptyState` title "No payouts with this status."
- **Error:** alert-error with API message + `Retry` ghost button (same
  sanction as §5.2).

**Detail (expanded row)** via the `Details` toggle, built **only from the
list row's data** (no fetch exists — S2): full settlement ID, merchant ID,
period start/end as full timestamps, gross/fee/net with currencies, status
badge, payout method, payout reference (full, with a `copyText()` copy button
from `lib/ui.ts` when non-null), `created_at`, and a one-line status
explanation from the §3.6 table. For `paid`, the line reads: "Paid out —
reference {payout_reference}." For `failed`/`on_hold`: "This payout needs
platform attention; there's nothing you need to do." (True: no merchant
action exists for either state.)

### 6.4 Actions that are deliberately NOT on this page

- **No "Mark paid" / Pay button.** Although `POST /settlements/:id/pay`
  currently accepts the owner (gap S5), recording a payout is the platform
  asserting it sent the merchant money; a merchant-side button would be
  self-attestation and corrupts the balance (§3.3). C4 renders no pay
  control for any status, and C2 hardens the endpoint to admin-only.
- **No fee editing, no status editing, no delete.** No endpoints exist.
- Admins who need Pay use the admin surface (out of Stream C scope).

### 6.5 Run-settlement form (the one write action)

Placement: a card **below** the list, heading "Run a settlement", collapsed
by default is *not* required — visible but visually secondary. Copy
(mandatory honesty points): "A run calculates settlements for completed
payments in a period that hasn't been settled yet. It creates payout records
— it does not send money. Payments already covered by an earlier settlement
period are skipped, so a period is never paid twice. The platform fee is
applied automatically; you can't change it here." (Last sentence is the UI
half of S4: the form never sends `fee_bps`.)

Fields:
| Field | id/type | Notes |
|---|---|---|
| Period start | `#run-start`, `type="date"` | Required |
| Period end | `#run-end`, `type="date"` | Required; client-check `end > start` with `fieldError()` ("Period end must be after period start.") |
| Quick ranges | three ghost buttons | "Last 7 days", "Last 30 days", "This month" — fill the two date inputs only |

Conversion (exact): the API takes RFC 3339 datetimes over the half-open
interval `[start, end)`. Send `period_start = new Date(startValue + "T00:00:00Z").toISOString()`
and `period_end = new Date(endValue + "T00:00:00Z").toISOString()` where the
end date input means "up to and including this day" in the UI label
("Period end (inclusive)") — i.e. the merchant picks the last day they want
covered and C4 adds one day before sending:
`period_end = startOfDayUTC(endValue) + 24h`. Label the inputs
"First day" / "Last day" to match this semantics and avoid the classic
off-by-one where the end day silently settles nothing.

Submit flow: `confirmAction()` from `lib/ui.ts` ("Create settlements for
{start} – {end}? Payments already settled for this period will be skipped.",
confirm label "Run settlement") → `withBusy()` on the submit button →
`POST /api/v1/settlements/run` with body exactly
`{ merchant_id, period_start, period_end }` (**no `fee_bps` key**).
On `201`: read `created` from the response — if `created > 0`, success toast
"Created {created} settlement(s) for this period." and reload list page 1;
if `created === 0`, info alert "No new settlements — either there were no
completed payments in this period, or they were already settled." (Both are
success outcomes; `0` must not render as an error.) On `403`/other errors:
inline error in the form card from the API `error` message.

---

## 7. Navigation & i18n (C3 lands, C4 consumes)

`frontend/src/components/DashboardNav.astro` — add two entries to the
`links` array, group `"Operate"`, immediately after `invoices` (keeps
money-flow order: Transactions → Payment Links → Invoices → Disputes → Payouts):

```ts
{ key: "disputes", label: "Disputes", href: "/dashboard/disputes", group: "Operate" },
{ key: "payouts",  label: "Payouts",  href: "/dashboard/payouts",  group: "Operate" },
```

Active-state derivation already works off `pathname.startsWith(l.href)`;
keys produce `data-i18n="nav_disputes"` / `data-i18n="nav_payouts"`
automatically via the existing `nav_${key.replaceAll("-","_")}` expression.

`frontend/src/lib/i18n.ts` — add beside the other `nav_*` keys:

```ts
nav_disputes: { en: 'Disputes', bn: 'বিরোধ' },
nav_payouts:  { en: 'Payouts',  bn: 'পেআউট' },
```

(Bengali strings are proposals for the user's review, following the existing
transliteration style in that file — e.g. `nav_api_keys` uses "API কী". If the
reviewer prefers transliteration: disputes "ডিসপিউট", payouts as above.)

No admin-nav changes; no changes to `/admin/*` in this stream.

---

## 8. Build order & acceptance checklist

1. **C2 first (API):** D1 (dispute filters), S3 (status parse), S4 (fee_bps
   gate), S5 (pay → admin-only); S1/S2 if time. Each with handler tests
   mirroring the existing disputes/settlements test style; existing tests
   must stay green (S5 will require updating any test that pays as owner —
   that test change is expected and should be called out in the PR).
2. **C3:** F1 badge entries + §7 nav/i18n + Disputes page (§5). Depends on
   nothing from C2 (workarounds specified), improves automatically when D1 lands.
3. **C4:** Payouts page (§6). Depends on C3 only for the `on_hold` badge (F1)
   and nav (already added by C3 — do not duplicate).

Acceptance checks for reviewers:
- [ ] No page sends `fee_bps`, renders a Pay control, a file `<input>`, or a
      fabricated deadline/next-payout date. (grep the two .astro files)
- [ ] Every list request carries explicit `page` and `per_page` (§1.5).
- [ ] Dispute evidence note form shows the replacement warning; file form
      shows the link-not-upload copy.
- [ ] Settlement net is always computed as gross − fee from the nested
      `Money` objects, never read from a `net` field (there is none).
- [ ] Status filter on Payouts uses the §3.1 value map until S3 is verified
      landed, then flips to snake_case.
- [ ] All statuses from §2.8/§3.6 render via `statusBadge` with the F1 entries.
- [ ] Empty/loading/error states match §5.2/§6.3 copy and the model page's
      element ids pattern (`…-message`, `…-loading`, `…-empty`, `…-table`,
      `…-pagination`).
