# 06 — Checkout UX Audit: Hosted Checkout (`/checkout/[id]`) + Payment Link (`/pay/[slug]`)

**Wave 151 — Design Engineering Team, Checkout UX Engineer seat**
**Scope inspected (audit only, no source edits):**
`frontend/src/pages/checkout/[id].astro`, `frontend/src/pages/pay/[slug].astro`,
`frontend/src/lib/api.ts` (calls used), `frontend/src/lib/i18n.ts`,
`frontend/src/layouts/BaseLayout.astro` (shared tokens / base styles)
**Test viewport assumed throughout:** 360 × 640 px Android, one thumb, intermittent
3G/4G, payer switching between this page and bKash/Nagad app or USSD.
**Payment reality assumed:** (a) manual bKash/Nagad — payer leaves page, does
Send Money in wallet app, returns with sender number + TrxID; (b) tokenized /
redirect gateways — payer is handed to provider page via `redirect_url`.

**API calls actually used by the payer surfaces (from `lib/api.ts`):**

| Page | Call | Endpoint | Auth |
|---|---|---|---|
| Checkout | `checkoutDetails(id)` | `GET /api/v1/checkout/:id` | none |
| Checkout | `submitCheckout(id, data)` | `POST /api/v1/checkout/:id/submit` — `{sender_number, provider_trx_id → transaction_id, gateway}` | none |
| Checkout | `apiFetch` (FX only) | `GET /api/v1/fx/rates` | required → silently fails for anonymous payers, selector stays hidden |
| Pay link | `publicLink(slug)` | `GET /api/v1/payment-links/public/:slug` | none |
| Pay link | `payLink(slug, data)` | `POST …/public/:slug/pay` — `{amount_minor, gateway, customer_phone?, customer_email?}` | none |
| — | `listGateways()` exists (`GET /api/v1/gateways`, `auth:false`) | **not called by either public page** | — |

No status-polling call is used on either page. That single fact drives several
P0 findings below (§1.6, §3.4).

---

## 0. Verdict in one paragraph + severity table

Both pages are clean, honest, single-card forms — and both break at the exact
moment of payment: **neither page ever tells the payer the merchant's wallet
number to send money to.** Checkout says “send the amount … using the payment
method above”; the pay-link success screen says “send the money with your
chosen payment app and use the reference above”. A first-time payer on a
360 px phone cannot complete a manual payment from these screens alone
without asking the merchant out-of-band. Everything else — no step indicator,
no gateway choice on checkout, a hard-coded gateway `<select>` on the pay
page, no copy buttons, no polling, amount that scrolls away — compounds that.

| # | Severity | Finding | Where |
|---|---|---|---|
| C-01 | **P0** | Merchant recipient wallet number never displayed (no field for it in the rendered UI; only free-text `instructions` *may* contain it) | both |
| C-02 | **P0** | Pay-link “success” ends the flow: no sender-number / TrxID capture, no fallback if SMS auto-match fails; reference falls back to the link `slug` | `pay/[slug].astro` |
| C-03 | **P0** | Checkout offers **no gateway selection**: gateway is pre-decided, rendered as a text row (`gatewayDisplay()` joins multiples with `, `); submit silently uses `gateways[0]` | `checkout/[id].astro` |
| C-04 | **P0** | Pay-link gateway list is hard-coded HTML (`bkash…surecash, manual`), not the merchant's enabled gateways — payer can pick a gateway the merchant never configured | `pay/[slug].astro` |
| C-05 | **P1** | No polling / refresh for `processing`: payer told “you can safely close this page”, given no way back to check status | checkout |
| C-06 | **P1** | No copy buttons for recipient number / amount / reference / TrxID — on 360 px, manual transcription of `9HX7K2LMNP`-style IDs is the #1 error source | both |
| C-07 | **P1** | No step indicator, no “you are here / what's next” on a genuinely multi-step (leave-app-and-return) flow | both |
| C-08 | **P1** | Amount not sticky and not repeated on the submit button (checkout) — payer must scroll up to re-check the exact poisha while in the wallet app | checkout |
| C-09 | **P1** | Validation is non-empty-only: no BD mobile format (`01[3-9]XXXXXXXX`, 11 digits), no TrxID format/length hint per gateway, errors are one generic top-of-form alert, not field-level | both |
| C-10 | **P2** | i18n is partial: hints, footnote, redirect copy, processing/submitted bodies, all JS error strings, FX label are hard-coded English; amounts always render `en-US` numerals (`BDT 1,234.56`) even in BN mode | both, `i18n.ts` |
| C-11 | **P2** | Touch targets: base `.btn` padding gives ≈40 px height (< 48 dp Material minimum); language toggle is a small floated button colliding with the centred brand line | `BaseLayout`, both |
| C-12 | **P2** | No fee / charge transparency: no Send-Money-vs-Cash-Out instruction, no “merchant bears fee / you pay exactly ৳X” line, no subtotal→total breakdown | both |

---

## 1. Step-by-step payer flow critique (mobile-first, 360 px)

### 1.1 Hosted checkout — `/checkout/[id].astro`

| Step | What payer experiences today | Critique |
|---|---|---|
| 0. Land / load | Text-only `Loading checkout…` card (`aria-live=polite` ✓). No skeleton, spinner, or merchant placeholder. | On slow 4G the page looks broken/blank-ish; no reassurance whose link this is. Fail → §3.4 error card with **no Retry button** — payer must know to reload. |
| 1. Identify merchant + amount | Header: 48 px logo (often absent — code comments admit the current API “returns only the merchant name”, branding applied “defensively”), `Pay` eyebrow, merchant `<h1>`, raw status badge (`Pending`). Summary rows: Amount due / Payment method / Expires. | Amount is `1.375rem` — **same size as the merchant name**. The single most important number is not the visual hero. No reference/order ID, no itemisation, description is one muted paragraph. Expiry is a locale datetime string, **not a countdown** (“expires in 12:04”) — no urgency, no expired-while-paying handling. |
| 2. Choose how to pay | *Does not exist.* Gateway is a read-only row. If several gateways exist they are comma-joined text; the form later submits with the **first** one. | See C-03 / §3.1. A bKash-only payer handed a Nagad checkout has no recourse except abandoning. |
| 3. Read instructions | One `.alert` box, heading “How to pay”, one paragraph — generic default text, or merchant's free-text `instructions` if set (`white-space: pre-line` ✓). | Not numbered steps. No recipient number (C-01), no “Send Money, not Cash Out”, no app-vs-USSD path, no fee note, no “come back here after paying” warning — yet leaving to the wallet app is the required next action, and Android may discard/reload this tab meanwhile (form state is **not persisted**; typed TrxID is lost on reload). |
| 4. Leave, pay in wallet app | Entirely off-page, unsupported. | No deep link (`bkash://` / app-open button), no “copy number / copy amount” before leaving (C-06), no summary the payer can keep on screen. This is where the flow actually happens and the design spends zero effort here. |
| 5. Return + enter sender number | `type=tel, inputmode=tel, autocomplete=tel` ✓, placeholder `01XXXXXXXXX`, one-line hint. | Good input semantics, then: no `pattern`, `maxlength=11`, or live formatting; `017…`, `+88017…`, and `88017…` all pass if non-empty (C-09). Hint is hard-coded English (C-10). No “this must be the number you sent *from*” emphasis beyond the hint — the classic mistake (entering the merchant's number) is not designed against. |
| 6. Enter TrxID | Text input, `autocapitalize=characters`, placeholder `e.g. 9HX7K2LMNP`, hint “shown in your payment confirmation / SMS”. | No per-gateway format (bKash TrxID = 10-char alphanumeric; Nagad differs), no auto-uppercase *styling* (`text-transform`), no paste affordance, no example SMS, no “where to find it” expander, no check-digit/length feedback before submit. One mistyped character = failed match = `processing` limbo (§1.1 step 8). |
| 7. Submit | Full-width button ✓, label “I have paid — submit details” (honest, good). Disabled on submit. | Button does **not repeat amount/gateway** (“I have paid ৳500 via bKash”); no loading label/spinner — on slow networks a frozen-looking disabled button invites back-button abandonment. Double-submit is prevented ✓, but there is no confirmation step (“You sent ৳500 from 017… — is that right?”), so a typo is submitted blind. |
| 8. Submitted / processing | `submitted-area` (success-styled) and `processing-area` are two near-identical messages: submitted → “will be confirmed automatically once the payment SMS is verified”; processing → “Awaiting confirmation … You can safely close this page”. | Confusing duplication: payer cannot tell if *submitted* ≠ *confirmed*. No expected wait (“usually under 5 minutes”), no TrxID/amount recap, no receipt, **no polling, no “Check status” button, no return-to-merchant button** (C-05). If SMS verification never matches, this page is a dead end with no support path. |
| 9. Completed | Green ✓ circle (3 rem), “Payment complete. Thank you — the merchant has been notified.” | The best state on either page — and still no amount, merchant, TrxID, timestamp, reference, screenshot/save affordance, or continue/return CTA. Payer has no proof-of-payment artefact to show the merchant in Messenger, which is how these disputes are actually settled in BD. |
| — Tokenized / redirect variant | If `redirect_url` present, a “Continue to payment” button + small muted explainer appears **above, in addition to**, the pending manual form. | Two competing payment paths on one screen with no either/or framing. No warning that leaving is expected, no auto-redirect, no “you'll come back here after paying” note, no provider branding on the button. A payer can reasonably fill the manual form *and* click redirect. |

### 1.2 Payment link — `/pay/[slug].astro`

| Step | What payer experiences today | Critique |
|---|---|---|
| 0. Land / load | `Loading payment link…` text card; error card is comparatively good (heading + API message + “ask the merchant for a fresh link” hint). | Same no-skeleton, no-retry issues as checkout. |
| 1. Identify + amount | Logo up to 220×56, merchant as small uppercase muted line, link title as `<h1>`, optional brand message. Fixed amount at `1.75rem/700` — the strongest amount treatment in the product — and the submit button repeats it: `Pay {amount}` ✓✓. | Model for checkout to copy. But merchant is visually *demoted* below the amount (trust order inverted, §2), and there is no expiry/uses-left cue even though links can expire / be exhausted. |
| 2. Enter amount (open-amount links) | `type=number, step=0.01`, hint “Enter the amount in BDT.” | No ৳ prefix adornment, no min/max from the link, no quick-amount chips, decimal keyboard invites poisha-level typos (`500.50` vs `50050`). Validation only `> 0`. |
| 3. Contact fields | Phone/email fields appear only when `collect_phone/email` is set, and become `required`. | No explanation *why* the merchant needs the phone (for matching the payment!) — the one field that could power SMS matching is presented as bureaucracy. No BD phone format validation (C-09). |
| 4. Choose gateway | Native `<select>`, bKash pre-selected by default. | See C-04 / §3.1 — hard-coded, unbranded, unordered by popularity, “Manual transfer” unexplained, and pre-selection manufactures consent for bKash. A `<select>` also hides fees/speed differences entirely. |
| 5. Submit | `Pay now` / `Pay ৳X`, full width, taller padding (0.75 rem) ✓, disabled on submit. | Same no-loading-label issue. |
| 6. “Payment started” success | Separate card: green alert “waiting for the money to arrive”, then Transaction ID + Reference (monospace) + Amount, then a paragraph: send money with chosen app, “use the reference above exactly as shown”, SMS auto-verifies, no screenshot needed. Keeps-note below. Respects `prefers-reduced-motion` on scroll ✓. | **Semantically this is a *pending* state dressed as success** (green, replaces the form, no further action possible). It never shows the number to send *to* (C-01); “use the reference” does not say *where* (bKash reference field? Nagad? USSD flow has none visible to many users); if no reference-carrying SMS reaches the merchant, there is **no TrxID fallback form, no link to `/checkout/…`, no status check** (C-02). Reference falls back to the public link slug — meaningless as a payment reference and confusing to “use exactly as shown”. No copy buttons for the two strings the payer must reproduce exactly (C-06). |

### 1.3 Cross-cutting mobile issues (360 px)

- **Container/card:** `max-width:560px`, card padding 1.5 rem (checkout) — fine
  at 360 px, no horizontal overflow found; `.checkout-row` flex with
  `overflow-wrap:anywhere` handles long gateway strings. This is not the problem.
- **Tap targets:** `.btn` ≈ 40 px tall, `.btn-small` language toggle smaller;
  both below the 48 dp guideline (C-11). The checkout submit is full-width ✓
  but not sticky — after scrolling past instructions + 2 fields it can sit
  below the fold with the amount long gone (C-08).
- **Input zoom / keyboards:** inputs inherit `0.9375rem` (15 px) — under the
  16 px iOS zoom threshold (secondary here, Android-first brief, but cheap to
  fix to 16 px). TrxID field lacks `inputmode`/monospace styling.
- **State loss:** neither page persists in-progress input (`sessionStorage`)
  or restores after the wallet-app round trip / tab reload.
- **FX selector:** an English-only “Display currency” field injected mid-summary
  on both pages when rates load — for anonymous payers it normally never
  appears (auth-gated endpoint). Dead weight in the payer path; if it ever
  does appear it competes with the real amount and risks “which amount do I
  send?” errors. Remove from payer surfaces or move below the summary, clearly
  labelled approximate.

---

## 2. Trust signals — present vs missing

### 2.1 Present (keep)

| Signal | Evidence |
|---|---|
| PIN-safety warning | Checkout footnote: “Never share your wallet PIN — you only need to enter the number you paid from and its transaction ID.” — the single best trust line in the product. |
| Hosted-checkout framing | “Secure hosted checkout by MiraclePay” footnote; consistent MiraclePay brand line at top of both pages. |
| Merchant branding hooks | Logo, display name over legal name, `checkout_message`, brand colour accenting the submit button (validated `#rrggbb`) — implemented on both pages (Waves 37/98). |
| Honest state language | “Payment started / waiting for the money to arrive” (pay), “Awaiting confirmation” (checkout) — the copy does not claim money arrived when it hasn't. |
| Safe rendering | All merchant-supplied strings set via `textContent`, never HTML — no merchant-injected markup on a page that asks for payment details. |
| Accessibility basics | Skip link, `:focus-visible`, `role=alert/status`, `aria-live` loading, reduced-motion handling (pay page scroll). |

### 2.2 Missing (in priority order)

1. **Merchant identity is unverifiable.** Name + optional logo only; no verified
   badge, no merchant phone/area, no “payments go to *<name>, wallet •••1234*”,
   no trade/KYC cue. Worse, on checkout the branding is defensive because the
   API currently returns only the merchant name — in production the logo/
   colour/message will usually be **absent**, leaving an anonymous card.
2. **The pay page has no PIN warning and no security footnote at all** — the
   warning exists only on checkout. Inconsistent protection exactly where
   the payer is about to open their wallet app.
3. **“Secure” is asserted, never shown.** No lock icon / HTTPS cue, no
   “MiraclePay never asks for your PIN or OTP” (OTP is the live BD scam —
   PIN alone is not the whole warning), no link to a verification page
   (“check you're on miraclepay…”), no support/report-scam path.
4. **Amount prominence is inconsistent.** Pay page: 1.75 rem + repeated on the
   button. Checkout: 1.375 rem, one row among three, never repeated. No
   breakdown (goods + delivery − discount = total), no fee bearer line, no
   “pay exactly this amount — a different amount cannot be verified” warning,
   which for SMS-matched manual payments is a functional requirement, not polish.
5. **No proof artefacts.** No reference/order number on checkout at any state,
   no downloadable/shareable receipt on completion, no timestamp.
6. **No human fallback.** No merchant contact, no MiraclePay support link, no
   “paid but still pending after 10 minutes?” path on any state. For a manual,
   SMS-verified system, the failure mode *is* the support case.
7. **Language trust gap.** Default EN, toggle is a small floated text button;
   half the strings stay English in BN mode (C-10) — mixed-language payment
   pages read as phishing to exactly the users BN mode exists for.

---

## 3. Gateway selection, instructions, and states

### 3.1 Gateway selection UX

- **Checkout: none.** Read-only text row; multiple gateways comma-joined;
  submission silently pins `gateways[0]`. Payer's wallet ≠ assigned gateway
  → abandon or pay wrong network and fail verification.
- **Pay link: wrong source of truth.** Six hard-coded `<option>`s regardless
  of merchant configuration or link settings; `listGateways()` is never
  consulted, and `publicLink`'s typed shape carries no gateway list at all —
  this is an API-contract gap as much as a UI gap (redesign §4.5).
- **Both:** no gateway brand marks (bKash pink / Nagad orange are instant
  recognition for BD users — text-only names waste that), no grouping of
  *Instant (redirect/tokenized)* vs *Manual (send & confirm)*, no per-gateway
  fee, limit, or “you'll need the app / USSD works” cue, no persistence of
  last-used gateway, no logos ⇒ mis-taps between similar names are invisible.

### 3.2 Instruction clarity for manual payments

The manual flow needs six facts; the pages reliably provide **one** (amount,
checkout only implicitly):

| Required fact | Checkout | Pay link (post-submit) |
|---|---|---|
| 1. Recipient wallet number (merchant's) | ✗ never rendered | ✗ never rendered |
| 2. Exact amount incl. poisha | △ in summary, not in instructions / button | △ in details list, not in the instruction sentence |
| 3. Send Money vs Cash Out / Payment | ✗ | ✗ |
| 4. Reference to include | n/a (TrxID model) | △ “use the reference exactly” — not told where to put it |
| 5. What to bring back (sender no. + TrxID) | △ form labels imply it, instructions paragraph doesn't enumerate it | ✗ no return step exists |
| 6. How/where TrxID is found | △ one hint line | n/a |

Sender-number entry: label + hint are clear in isolation (“The mobile wallet
number you sent the payment from”) but nothing prevents the two classic
errors — entering the *recipient's* number, or a number in `+880` format that
backend matching may not normalise. TrxID entry: no gateway-specific format,
example, or find-it help (see §1.1 step 6). Neither field offers paste-friendly
affordances (monospace, letter-spacing, auto-uppercase display) or a
review/confirm step.

### 3.3 Error states

- **Load error (both):** single card, “!” icon (checkout) / API message (pay),
  “ask the merchant for a new link”. No Retry, no distinction between
  *expired* (merchant's fault, get new link), *not found*, and *network*
  (payer's connection, retry) — three different recoveries collapsed into one.
  Checkout's expired case routes here too, discarding merchant/amount context.
- **Form error (both):** one `role=alert` banner + focus to the offending
  field (checkout) — acceptable skeleton, but messages are presence-only
  (C-09), raw API error strings are surfaced verbatim to payers (checkout
  submit + pay load), and errors are not bound to fields
  (`aria-describedby` / `aria-invalid` absent).
- **No offline / double-payment guard:** nothing warns “if you already paid,
  don't pay again — enter the same TrxID”, the highest-stakes error copy for
  manual payments, on either page.

### 3.4 Success / pending / failure states

| State | Checkout | Pay link | Gap |
|---|---|---|---|
| Pending (pre-payment) | form + instructions | form | no steps, no countdown |
| Submitted / processing | two overlapping messages (§1.1.8) | “Payment started” (green) | merge into one honest *Verifying* state with timeline + polling |
| Completed | ✓ circle + thanks | — (never reaches it in-page) | no receipt data, no return CTA |
| Failed / cancelled / refunded | one generic `status-area`: “no longer awaiting payment… contact the merchant” | n/a | refunded payers get the same text as failed ones; no reason, no next step, no support link |
| Expired | load-error card | load-error card | loses context; no merchant contact |

---

## 4. Proposed redesign spec

Design targets: 360 px first; every action reachable by thumb; payer never
has to memorise a number across an app switch; EN + BN parity for **every**
string, including errors and JS-generated text.

### 4.1 Layout (both pages converge on one template)

```
┌────────────────────────────────────┐
│ MiraclePay 🔒 Secure checkout  [বাং]│  trust bar, 48px, sticky
│ ┌────────────────────────────────┐ │
│ │ [logo] Merchant Name        ✓  │ │  verified merchant header
│ │        Verified merchant       │ │
│ └────────────────────────────────┘ │
│  ● Method ─── ● Pay ─── ○ Confirm  │  step indicator (§4.2)
│ ┌────────────────────────────────┐ │
│ │ PAYMENT METHOD                 │ │
│ │ ( ◉ ) [bKash logo] bKash       │ │  radio cards, ≥56px tall,
│ │       Send Money · No fee      │ │  brand colour left border
│ │ ( ○ ) [Nagad logo] Nagad       │ │
│ │ ( ○ ) Card / Bank  → redirect  │ │  grouped: Manual | Instant
│ └────────────────────────────────┘ │
│ ┌────────────────────────────────┐ │
│ │ HOW TO PAY WITH BKASH          │ │
│ │ 1 Send Money to:               │ │
│ │   01712-345678        [ Copy ] │ │  ≥20px mono, copy button
│ │ 2 Amount: ৳500.00     [ Copy ] │ │
│ │ 3 Come back here and enter     │ │
│ │   your number + TrxID          │ │
│ │ [ Open bKash app ] (if avail.) │ │
│ └────────────────────────────────┘ │
│  Your bKash number [01_________]   │
│  Transaction ID    [__________]    │  mono, auto-uppercase
│  [ I have paid ৳500.00 — Confirm ] │
│  Never share PIN/OTP… (§4.6)       │
├────────────────────────────────────┤
│ STICKY SUMMARY: ৳500.00 · bKash ▸  │  (§4.3) always visible
└────────────────────────────────────┘
```

Rules: one card column, 16 px gutters; all inputs 16 px font, ≥48 px tall;
all buttons ≥48 px; radio gateway cards ≥56 px; instructions are a numbered
list, never a paragraph; recipient number and amount each get a Copy button
(`navigator.clipboard` + “Copied ✓” feedback, fallback select-on-tap);
in-progress form persisted to `sessionStorage` and restored on return.

**Pay-link page adopts the same template:** gateway radio cards replace the
`<select>`; on submit it transitions to Step 3 (instructions + sender/TrxID
form) *in the same page* instead of terminating at “Payment started” — the
current success card becomes the *Verifying* state (§4.4), reached only after
details are submitted or SMS match is detected.

### 4.2 Step indicator

Three steps, labelled, with the current step announced (`aria-current=step`):

1. **Method** — `EN: Method` / `BN: মাধ্যম`
2. **Pay** — `EN: Pay` / `BN: পেমেন্ট`
3. **Confirm** — `EN: Confirm` / `BN: নিশ্চিত করুন`

Compact on 360 px: numbered dots + labels, completed = check, connector line
fills with `--brand`. Checkout with a pre-assigned single gateway starts at
Step 2 with Step 1 shown completed. Redirect/tokenized gateways: Step 2 is
“Pay on {provider}”, Step 3 auto-completes on return/webhook.

### 4.3 Sticky pay summary

Fixed bottom bar (above Android gesture area, `env(safe-area-inset-bottom)`):
left = total `৳500.00` (20 px, 700) + gateway name/logo; right = expand
chevron opening a bottom sheet with breakdown (Subtotal / Delivery / Discount
/ **Total to pay**), merchant, reference, expiry countdown. The primary CTA
lives **inside** the sticky bar on Steps 2–3 so amount + action are never
separated. Bar hides when the on-screen keyboard is open if viewport height
collapses (`interactive-widget` / resize guard) to avoid covering inputs.

### 4.4 States redesign

- **Loading:** skeleton of header + summary + 3 gateway rows; >8 s → inline
  “Still loading — check connection [Retry]”.
- **Verifying (replaces submitted/processing/“payment started”):** spinner +
  “We're checking the payment SMS — usually under 5 minutes.” Poll
  `checkoutDetails` every 10 s (pause when tab hidden), show submitted
  recap (amount, sender •••last-4, TrxID), [Check now] button, “You can close
  this page — bookmark/save this link to check later.” After 10 min:
  escalate copy + merchant contact + “Paid but not matched? Send details”
  fallback (re-open TrxID form).
- **Completed:** green check, full receipt card (merchant, amount, gateway,
  TrxID, date/time, reference) + [Save / Share receipt] (Web Share / image)
  + [Return to merchant] when a return URL exists.
- **Failed / expired / cancelled / refunded:** distinct icon, plain-language
  reason, one primary recovery each (Retry payment / Get a new link from
  merchant / Contact merchant), never a raw API string.
- **Field errors:** inline under the field, `aria-invalid` +
  `aria-describedby`, format-specific (§4.6); banner reserved for
  server/network failures, with human-mapped messages.

### 4.5 Data / API prerequisites (flagged, not designed around)

1. Public checkout + public link payloads must expose per-gateway
   **recipient instructions**: `{gateway, recipient_number, recipient_name,
   transfer_type: send_money, reference?, instructions?}` — without this,
   C-01 cannot be fixed in UI alone.
2. `publicLink` must return the merchant's **enabled gateways** (id, display
   name, type manual/redirect, fee note) — fixes C-04 at the source; delete
   the hard-coded `<option>` list.
3. Checkout payload should expose **all** eligible gateways for payer choice
   (C-03) plus `return_url`, merchant contact, and a payer-safe `reference`.
4. A public status read already exists (`checkoutDetails`) — polling needs
   no new endpoint, only client wiring (§4.4).

### 4.6 Copy deck — EN + BN (all strings via `i18n.ts` keys; BN numerals for amounts in BN mode)

| Key / placement | EN | BN |
|---|---|---|
| Step 1 / 2 / 3 | Method · Pay · Confirm | মাধ্যম · পেমেন্ট · নিশ্চিত করুন |
| Trust bar | Secure checkout by MiraclePay | MiraclePay-এর নিরাপদ চেকআউট |
| Verified merchant | Verified merchant | যাচাইকৃত বিক্রেতা |
| Sticky total label | Total to pay | মোট প্রদেয় |
| Instructions heading | How to pay with {gateway} | {gateway} দিয়ে কীভাবে পেমেন্ট করবেন |
| Step 1 | Send Money to this number: | এই নম্বরে সেন্ড মানি করুন: |
| Step 2 | Send exactly {amount}. A different amount can't be verified automatically. | ঠিক {amount} পাঠান। অন্য পরিমাণ স্বয়ংক্রিয়ভাবে যাচাই করা যাবে না। |
| Step 3 | Then come back here and enter your number and Transaction ID below. | তারপর এখানে ফিরে এসে নিচে আপনার নম্বর ও লেনদেন আইডি দিন। |
| Send Money warning | Use Send Money — not Cash Out. | সেন্ড মানি করুন — ক্যাশ আউট নয়। |
| Copy / Copied | Copy / Copied ✓ | কপি করুন / কপি হয়েছে ✓ |
| Sender label | Your {gateway} number (the one you paid from) | আপনার {gateway} নম্বর (যে নম্বর থেকে পাঠিয়েছেন) |
| Sender error | Enter the 11-digit number you paid from, e.g. 01712345678. | যে নম্বর থেকে পাঠিয়েছেন সেই ১১-সংখ্যার নম্বরটি দিন, যেমন ০১৭১২৩৪৫৬৭৮। |
| TrxID label | Transaction ID (TrxID) | লেনদেন আইডি (TrxID) |
| TrxID help | Find it in the confirmation SMS from {gateway}. | {gateway}-এর নিশ্চিতকরণ SMS-এ এটি পাবেন। |
| TrxID error | That TrxID doesn't look right — bKash TrxIDs are 10 letters/numbers. Check the SMS and try again. | TrxID-টি ঠিক মনে হচ্ছে না — বিকাশ TrxID হয় ১০ অক্ষর/সংখ্যার। SMS দেখে আবার চেষ্টা করুন। |
| Primary CTA (manual) | I have paid {amount} — confirm | আমি {amount} পাঠিয়েছি — নিশ্চিত করুন |
| Primary CTA (redirect) | Pay {amount} with {gateway} | {gateway} দিয়ে {amount} পেমেন্ট করুন |
| PIN/OTP warning (both pages) | MiraclePay will never ask for your wallet PIN or OTP. Only enter your number and TrxID here. | MiraclePay কখনো আপনার ওয়ালেট PIN বা OTP চাইবে না। এখানে শুধু আপনার নম্বর ও TrxID দিন। |
| Already-paid guard | Already paid? Don't pay again — enter the same TrxID. | ইতিমধ্যে পাঠিয়েছেন? আবার পাঠাবেন না — একই TrxID দিন। |
| Verifying heading | Payment submitted — verifying | পেমেন্ট জমা হয়েছে — যাচাই চলছে |
| Verifying body | We're matching the payment SMS. This usually takes under 5 minutes. You can close this page and return with the same link. | আমরা পেমেন্ট SMS মিলিয়ে দেখছি। সাধারণত ৫ মিনিটের কম সময় লাগে। পেজটি বন্ধ করে একই লিংক দিয়ে ফিরে আসতে পারেন। |
| Completed heading | Payment complete | পেমেন্ট সম্পন্ন |
| Completed body | {amount} received by {merchant}. Thank you. | {merchant} {amount} পেয়েছেন। ধন্যবাদ। |
| Expired heading | This link has expired | এই লিংকটির মেয়াদ শেষ |
| Expired body | Ask {merchant} for a new payment link. If you already paid, contact the merchant with your TrxID. | {merchant}-এর কাছ থেকে নতুন পেমেন্ট লিংক নিন। ইতিমধ্যে পাঠিয়ে থাকলে TrxID দিয়ে বিক্রেতার সঙ্গে যোগাযোগ করুন। |
| Load error + retry | We couldn't load this payment. Check your connection and try again. [Retry] | এই পেমেন্টটি লোড করা যায়নি। সংযোগ দেখে আবার চেষ্টা করুন। [আবার চেষ্টা করুন] |

### 4.7 Acceptance criteria for the redesign

- [ ] A payer can complete a manual bKash payment using **only** this page
      (recipient number, amount, transfer type all present, all copyable).
- [ ] Gateway choice on both pages comes from merchant configuration;
      checkout no longer submits an unseen `gateways[0]`.
- [ ] Amount visible at every scroll position (sticky bar) and repeated on
      the primary CTA; BN mode renders amounts/labels fully in Bangla
      (incl. errors) with Bangla numerals.
- [ ] Verifying state polls and resolves to Completed/Failed without reload;
      completed state yields a shareable receipt.
- [ ] All targets ≥48 px; form survives wallet-app round trip (state restored).
- [ ] Pay-link flow offers TrxID fallback when SMS matching hasn't confirmed
      within the stated window.

*End of audit — Checkout UX Engineer, Wave 151.*
