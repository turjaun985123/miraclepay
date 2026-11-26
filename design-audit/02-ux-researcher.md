# 02 — UX Researcher Audit (Wave 151, Design Audit)

**Scope / method:** Code-walk of the three key journeys through the Astro frontend only — no live
backend, no user interviews, no analytics. Every claim is grounded in a file read on 2026-11-26.
Pages walked: `index.astro`, `login.astro`, `register.astro`, `dashboard/index.astro`,
`dashboard/payment-links.astro`, `dashboard/gateways.astro`, `dashboard/transactions.astro`,
`dashboard/invoices.astro`, `dashboard/webhooks.astro`, `dashboard/api-keys.astro`,
`dashboard/settings.astro`, `pay/[slug].astro`, `checkout/[id].astro`, `admin/*`,
`components/DashboardNav.astro`, `lib/api.ts`. Step counts count distinct screens + required
form submissions a first-time user must complete, not clicks inside a form.

**Headline verdict:** The happy paths exist and are competently built (real loading / empty /
error states on most dashboard lists, Bangla toggle on payer pages, copy-button on payment
links). The product fails at the *seams*: onboarding never tells the merchant that a gateway
must be configured before money can arrive, the payer's two entry points (`/pay` vs
`/checkout`) ask for different things in a different order, and two daily-ops jobs the
marketing page promises — disputes and payouts — have **no UI at all**.

---

## Journey 1 — Merchant onboarding: register → dashboard → first payment link

### Steps (7, of which 3 are undiscoverable without exploration)

| # | Step | Where |
|---|------|-------|
| 1 | Landing → "Get started" | `index.astro` hero / nav CTA |
| 2 | Register (name, email, password) → auto-login → redirect | `register.astro` submit handler: `register()` then a second `login()` call, then `window.location.href = '/dashboard'` |
| 3 | Create first merchant (business name + optional email / country / phone / webhook URL) | `dashboard/index.astro` `#create-merchant-section` (only shown when `listMerchants()` returns `[]`) |
| 4 | Configure a gateway (merchant number, API key/secret, enable) — **required for real money, never presented as a step** | `dashboard/gateways.astro` — separate nav destination |
| 5 | (Optional but gating in practice) Submit KYC (business type, NID, address) | Buried at the bottom of `dashboard/index.astro`, below Search, Chart, Create payment and Recent transactions |
| 6 | Create payment link (title required; amount / currency / max-uses / collect phone-email optional) | `dashboard/payment-links.astro` `#create-form` |
| 7 | Find the URL in the list below and Copy it | `dashboard/payment-links.astro` `renderLinks()` — Copy button per row |

There *is* an onboarding checklist (`dashboard/index.astro` `loadOnboarding()`, Wave 115,
`GET /merchants/:id/onboarding`) — but see F1.3: it is display-only.

### Friction points (with file evidence)

- **F1.1 — No gateway-setup gate or warning anywhere in the activation path.** Register's lede
  says only "create your merchant from the dashboard to start accepting payments"
  (`register.astro`). The dashboard's create-merchant success path is `window.location.reload()`
  (`dashboard/index.astro`). A merchant can then create a payment link or a payment immediately:
  `dashboard/index.astro`'s Create-payment form populates its gateway `<select>` from the
  *public catalogue* `listGateways()`, not from the merchant's configured gateways
  (`merchantGateways()` is never called on that page), and `payment-links.astro` never checks
  gateway configuration either. First real payer therefore hits an unconfigured gateway.
- **F1.2 — Double round-trip + fragile seam at registration.** `register.astro` calls
  `register()` and then `login(email, password)`; if the second call returns no token the user
  gets "Registration succeeded but sign-in returned no token. Please sign in." — an account
  exists but the user is stranded on the register form with no link state change. No email
  verification step or "check your inbox" state exists anywhere (grep: no verification UI in
  `frontend/src`).
- **F1.3 — Onboarding checklist is a dead end.** `loadOnboarding()` renders each step as a
  `Done` / `To do` badge + label text only — no link, button, or anchor to the page that
  completes the step. It also silently hides itself on any fetch error (`catch { card.hidden =
  true }`), so a broken checklist is indistinguishable from a completed one. It sits below the
  balance cards on an already 988-line dashboard home that also contains Search, a 14-day
  chart, Create payment, Recent transactions, KYC and 2FA — the new merchant must scroll past
  five unrelated cards to find KYC/2FA.
- **F1.4 — No inline validation anywhere; errors arrive only after submit.** Every form uses
  `novalidate` (`register.astro`, `login.astro`, `dashboard/index.astro`,
  `payment-links.astro`, `checkout/[id].astro`, `pay/[slug].astro`). Register validates
  nothing client-side before submit (not even the stated "at least 8 characters, with a
  number" rule — the hint is prose; only the server can reject it), and errors render in a
  single top-of-form `role="alert"` paragraph, not against the offending field. No show/hide
  password control, no password-strength feedback, no terms/privacy acceptance on register.
- **F1.5 — Login is a trap for the locked-out.** `login.astro` has no "Forgot password" link
  (grep: zero matches for `forgot` in `frontend/src/pages`), despite a password-reset API
  existing in the backend wave history. The 2FA field is hidden until the server returns a
  `totp_required` / `totp_invalid` error string the page string-matches on — a user who knows
  they have 2FA still must fail once to see the field. Button colour also changes identity per
  page: register submit is teal `#0f766e`, login submit is blue `#2563eb`, landing CTAs are
  purple `#6d28d9` — three brand primaries in the first three screens.
- **F1.6 — Merchant context is invisible and inconsistent.** The active merchant lives in
  `localStorage 'mp_merchant'`; the switcher exists on `dashboard/index.astro` and
  `settings.astro` but **not** on transactions / payment-links / gateways / invoices, which
  silently resolve "stored id or first merchant" (e.g. `transactions.astro resolveMerchant()`,
  `payment-links.astro resolveMerchant()`). A two-merchant user can create a link under the
  wrong merchant with no on-page indication except payment-links' small "Merchant: …" line.
- **F1.7 — Link creation success doesn't hand over the link.** On create, `payment-links.astro`
  shows `Payment link "…" created.` and reloads the list; the user must find the new row
  (unsorted guarantee unstated) and press Copy. No success panel with the URL, no share
  (WhatsApp/Messenger) action — despite the landing page selling exactly that sharing story
  (`index.astro`: "share it anywhere — Messenger, WhatsApp, SMS").
- **F1.8 — No deactivation for links.** The page header comment states it plainly:
  `api.ts` exposes no deactivate/delete for payment links, "so this page deliberately offers
  no deactivation button." A mistyped-amount live link can only be left running until max-uses
  or expiry (if the creator happened to set them).

### Missing states — Journey 1

| State | Status |
|---|---|
| Loading | Present: "Loading your dashboard…" (`dashboard/index`), "Loading payment links…" — text-only; no skeleton/spinner/`aria-busy` anywhere in `frontend/src` |
| Empty | Present and good: create-merchant section, `#list-empty` on payment-links, `tx-empty` |
| Error | Present but generic: raw server `err.message` surfaced verbatim (from `api.ts apiFetch`, which throws `body.error`); no retry button on dashboard load failure — refresh is the only recovery |
| Success | Weak: text banners only; no next-step CTA after merchant creation (page reload), link creation (see F1.7), KYC submission, or gateway save beyond a banner |
| Email-verification / pending-verification | **Absent entirely** |
| Session-expired | **Absent**: a failed refresh clears the token in `api.ts tryRefresh()`, but dashboard pages just render the API error; only a missing token at page load redirects to `/login` |

### Drop-off risks — Journey 1

1. **Register → merchant → (hidden) gateway gap (F1.1/F1.3):** highest risk. The merchant
   believes they are "live" after step 3; the first payer failure teaches them otherwise.
2. **KYC discoverability (F1.3):** if payouts are KYC-gated server-side, merchants will hit a
   wall with the form five cards below the fold of the busiest page in the product.
3. **Post-registration sign-in seam (F1.2)** and **no password recovery (F1.5)**: classic
   permanent-lockout churn for a self-hosted tool with no support desk.

---

## Journey 2 — Payer journey: pay page → checkout → gateway → success/failure

There are **two parallel payer journeys that behave differently**, and nothing explains which
one a payer is on:

- **A. Payment link** — `pay/[slug].astro`: pick gateway from a `<select>` → "Pay now" →
  "Payment started" panel. The payer is *then* told to go send money from their wallet app
  using the reference. No sender-number / transaction-ID is ever collected on this page.
- **B. Hosted checkout** — `checkout/[id].astro`: read instructions → go send money first →
  come back and type wallet number + transaction ID → submit → "Payment submitted / Awaiting
  confirmation".

### Steps

| Journey | Steps |
|---|---|
| A (`/pay`) | 1 load link → 2 (open-amount only) type amount → 3 (if merchant enabled) phone/email → 4 choose gateway → 5 Pay now → 6 leave page, open wallet app, send money with reference → 7 wait, with no way to check status |
| B (`/checkout`) | 1 load checkout → 2 read amount/instructions → 3 leave page, open wallet app, send money → 4 return → 5 type sender number → 6 type transaction ID → 7 submit → 8 land in submitted/processing state |

### Friction points (with file evidence)

- **F2.1 — Checkout never shows the number to send money to.** `checkout/[id].astro` renders
  merchant name/logo, amount, gateway *name* (`gatewayDisplay()`), and expiry. The default
  instruction text is "Send the amount shown above to the merchant using the payment method
  above" — but no receiving wallet number/account appears anywhere in the markup; it only
  exists if the merchant's gateway config supplied custom `instructions`, which then replace
  the generic text. For the manual/send-money model that is the product's core (landing
  `index.astro` step 2), the single most-needed datum is missing by default.
- **F2.2 — `/pay` success is a false summit.** `pay/[slug].astro`'s success panel is headed
  "Payment started" and honestly explains the money still has to be sent — but it provides no
  copy button for the reference/transaction ID it tells the payer to "use exactly as shown"
  and to "keep … until the payment is confirmed", no status check, no polling, and no return
  path. The gateway `<select>` is also hard-coded to six options (bkash, nagad, rocket, upay,
  surecash, manual) regardless of which gateways the merchant actually configured — a payer
  can "pay" via a gateway the merchant never set up.
- **F2.3 — No live status anywhere for payers.** Neither payer page polls: the only
  `setInterval` in the frontend is the dashboard's 30 s refresh (`dashboard/index.astro`).
  Checkout's `processing` state says "You can safely close this page" — true, but a payer who
  stays sees a static page forever; a payer who leaves has no URL/bookmark story to check
  confirmation later. Expiry is rendered as a locale date string (`expires_at` →
  `toLocaleString()`), with no countdown and no "time left" warning before it lapses mid-payment.
- **F2.4 — Failure has no recovery path.** Checkout's terminal-status area
  (`#status-area`) shows "This checkout is no longer awaiting payment… contact the merchant" —
  no retry, no "start a new payment", no merchant contact detail (no email/phone is rendered
  on either payer page). A failed `submitCheckout` keeps the form (good) but the raw server
  error is the only guidance; a mistyped transaction ID and a genuinely unmatched payment look
  identical. `/pay` load failure collapses inactive/expired/exhausted/unknown into one card
  whose only advice is "ask the merchant for a fresh link".
- **F2.5 — Trust and safety signals are thin at the moment of money movement.** Checkout has a
  good footnote ("Never share your wallet PIN…"), but neither payer page shows: merchant
  contact, a verifiable merchant identity beyond a name/logo, fee/total breakdown (amount due
  is a single figure), or what happens if verification fails. The FX "display currency"
  selector on both pages calls an auth-required endpoint (`/api/v1/fx/rates`) that the code
  itself notes fails for anonymous visitors — dead weight that silently never appears for
  exactly the audience it targets.
- **F2.6 — Form ergonomics on the highest-stakes form.** Checkout's transaction-ID field has
  helpful hints and `autocapitalize`, but no paste-friendly formatting, no length/format
  validation before submit, and errors are a single banner (not field-level). Both payer
  pages do get credit: Bangla/English toggle, `prefers-reduced-motion` handling on `/pay`
  success scroll, large amount typography, merchant branding (logo/colour/message, Wave 37/98).

### Missing states — Journey 2

| State | Status |
|---|---|
| Loading | Present, text-only ("Loading checkout…" / "Loading payment link…") |
| Error (bad/expired link) | Present, single undifferentiated card; expired-during-session not handled (no timer) |
| Processing / pending-confirmation | Present as static panels; **no auto-refresh, no polling, no "check again" button** |
| Success (completed) | Present on checkout (`completed-area`); on `/pay` the terminal state shown is only "started", never "confirmed" |
| Failure (failed/cancelled/refunded) | Present as generic status area; **no retry / new-payment / contact action** |
| Partial / wrong-amount / unmatched-TrxID | **Absent** — no state or copy for the most common send-money failure modes |
| Offline / submit-timeout | **Absent** — a failed fetch and a rejected submission surface identically as the server/generic message |

### Drop-off risks — Journey 2

1. **F2.1 (no destination number)** is a hard blocker, not friction: payers literally cannot
   complete the default flow without merchant-supplied instructions.
2. **Leaving to the wallet app and returning** (both journeys) with no deep-link back, no
   reference copy affordance, and (journey A) the reference shown only *after* "Pay now".
3. **Static processing + no status lookup (F2.3):** payers will re-pay, abandon, or charge
   back socially (message the merchant) — support load lands on the merchant.
4. **Unconfigured-gateway selection on `/pay` (F2.2)** manufactures failed payments.

---

## Journey 3 — Merchant daily ops: transactions, disputes, payouts

### Steps

| Job | Steps today |
|---|---|
| Check today's money | 1 login → 2 dashboard (balance cards + 14-day chart + recent tx, auto-refresh 30 s) — the strongest flow in the product |
| Find/explain a transaction | 1 nav → Transactions → 2 filter by status dropdown and/or **free-text** gateway field → 3 scan 8-column table → 4 (completed only) Refund → inline panel → confirm |
| Handle a dispute | **Impossible in the UI — no disputes page, list, or detail exists** |
| Get paid out | **Impossible in the UI — no payouts page, history, or withdraw/request action exists** |

### Friction points (with file evidence)

- **F3.1 — Disputes do not exist for merchants.** Grep across `frontend/src`: "dispute"
  appears only as webhook event names in a reference table (`dashboard/webhooks.astro`:
  `dispute.opened`, `dispute.closed`) and in the backend wave history. `DashboardNav.astro`'s
  nine links contain no Disputes entry; `lib/api.ts` has no dispute type or function. A
  merchant learns of a dispute only if they run a webhook consumer or read raw events.
- **F3.2 — Payouts are a settings field, not a workflow.** `dashboard/settings.astro` offers a
  payout *schedule* (manual/daily/weekly/monthly + day + auto-settle threshold expressed in
  raw minor units — "10000 = 100.00" per its own hint). But there is no payout destination
  (bank/wallet account) form anywhere, no payout history, no next-payout date/amount, and no
  withdraw button — while `dashboard/index.astro`'s balance card teases "Available balance —
  Ready to withdraw" with no action attached. The landing page promises "Track … balances,
  and payouts per gateway and per currency" (`index.astro` Settlements feature) — the UI
  delivers none of that per-gateway/per-currency breakdown.
- **F3.3 — Transactions list is shallow for daily reconciliation.** `transactions.astro`:
  filters are status + a free-text gateway input (type `bkash` wrong and you get the empty
  state, with no suggestion); no date range, no search by reference/customer/TrxID (dashboard
  home has a global search, transactions does not), no pagination controls despite
  `ListTransactionsParams` supporting `limit/offset/page` in `api.ts`, no export/CSV, no row
  detail view — the row truncates the ID to 8 chars (`shortId()`) and the full record
  (fees, net, metadata, timeline) is unreachable. Empty state text conflates "no transactions
  ever" with "filters matched nothing" ("No transactions match these filters yet.").
- **F3.4 — Refund is the only money-out action, and it's undiscoverable-ish.** Refund appears
  only as a ghost button on `completed`/`partially_refunded` rows; the Wave 99 inline panel
  (amount prefilled to remaining refundable, optional reason) is good, but there is no
  confirmation step, no refund status tracking afterwards, and no partial-refund history on
  the row. No equivalent actions exist for pending transactions (no cancel, no resend
  reminder, no "mark as paid" for manual verification fallbacks).
- **F3.5 — Everything is a full-page silo; no cross-linking.** Dashboard recent-transactions
  rows are not clickable to the Transactions page; search results on dashboard home render as
  plain text list items (id — status · amount), not links; invoice rows can't jump to their
  transaction; webhook deliveries can't jump to their payment. Daily ops becomes tab-hopping
  and manual ID copying — except IDs are truncated in the UI.
- **F3.6 — Admin is in every merchant's nav.** `DashboardNav.astro` renders an "Admin" link
  to `/admin` for all users; non-admins discover their status only from the admin page's
  "Admin access required" section. Conversely, genuinely daily admin-adjacent work a merchant
  admin needs (KYC queue) lives only in `admin/index.astro`, far from the merchant KYC form.
- **F3.7 — Error/loading pattern is per-page bespoke.** Each page re-implements
  `showError`/`showMessage` with raw server messages and different placements (banner top,
  in-card, toast-like `#tx-message` that never auto-dismisses). A failed transactions load
  replaces the table with an error banner and no Retry button; the merchant's only recovery
  is re-submitting the filter form or reloading.

### Missing states — Journey 3

| State | Status |
|---|---|
| Loading | Present (text) on transactions/invoices/webhooks lists |
| Empty | Present on transactions/invoices/webhooks/payment-links; transactions empty copy doesn't distinguish first-run vs filtered-out (F3.3) |
| Error | Present, no retry affordance, raw API messages |
| Disputes (list/detail/respond/evidence) | **Absent entirely** |
| Payouts (history, pending, failed, destination setup, withdraw) | **Absent entirely** (schedule config only) |
| Transaction detail / timeline | **Absent** |
| Export / reconciliation report | **Absent** |
| Bulk actions, saved filters, date-range presets | **Absent** |

### Drop-off risks — Journey 3

1. **First dispute or first payout expectation (F3.1/F3.2):** the two moments a payment
   platform earns or loses trust have no UI; merchants will conclude the self-hosted instance
   is broken and churn to a hosted competitor.
2. **Reconciliation pain (F3.3/F3.5)** grows linearly with volume — the merchants who succeed
   on MiraclePay are the first to outgrow its ops UI.
3. **Silent wrong-merchant context (F1.6)** in daily ops risks refunds issued against the
   wrong store's transactions.

---

## Top 10 UX fixes, ranked by impact

| # | Fix | Journeys | Evidence |
|---|-----|----------|----------|
| 1 | **Show the receiving number/account on checkout by default** — render the configured gateway's merchant number as a first-class, copyable field in the "How to pay" block; block publishing a checkout whose gateway has no receiving number or instructions. | 2 | F2.1 — `checkout/[id].astro` renders gateway name only |
| 2 | **Gate activation on gateway configuration** — onboarding checklist becomes the activation flow: merchant → gateway → (KYC) → first link, each step a deep link; warn on payment/link creation when no enabled gateway exists; dashboard Create-payment lists only *configured* gateways. | 1 | F1.1, F1.3 — `dashboard/index.astro loadOnboarding()` (no links), `listGateways()` vs `merchantGateways()` |
| 3 | **Build the Disputes UI** — nav entry, list with status/due-date, detail with evidence upload + respond action; surface `dispute.opened` as a dashboard alert badge, not just a webhook. | 3 | F3.1 — disputes exist only in `webhooks.astro` event table |
| 4 | **Build the Payouts UI** — payout destination setup, history (pending/paid/failed), next scheduled payout, and a "Withdraw" action behind the dashboard's "Ready to withdraw" balance; express thresholds in major units. | 3, 1 | F3.2 — `settings.astro` schedule-only; `dashboard/index.astro` action-less balance card |
| 5 | **Live payer status** — poll checkout/`pay` status (e.g. 5–10 s) through processing → completed/failed, add a "Check status" + copyable reference on `/pay` success, and a payer-bookmarkable status view. | 2 | F2.2, F2.3 — no `setInterval` outside dashboard |
| 6 | **Failure recovery on payer pages** — failed/expired states get "Try again / start a new payment", the merchant's contact, and field-level guidance for unmatched TrxID; split expired vs inactive vs exhausted error copy. | 2 | F2.4 — `checkout/[id].astro #status-area`, `pay/[slug].astro` single error card |
| 7 | **Unify the two payer journeys' contract** — `/pay` collects sender number + TrxID after send-money (like checkout) *or* clearly defers it with instructions; restrict `/pay` gateway list to the merchant's enabled gateways instead of the hard-coded six. | 2 | F2.2 — `pay/[slug].astro` hard-coded `<select>`, no TrxID fields vs `checkout/[id].astro` form |
| 8 | **Fix auth dead-ends** — "Forgot password" on login, show-password toggle, inline field validation + strength meter on register, surface the 2FA field proactively (checkbox/link "I have a code"), and one brand primary colour across landing/login/register. | 1 | F1.4, F1.5 — `novalidate` everywhere; teal/blue/purple primaries |
| 9 | **Transactions as a work surface** — date-range + text search (reference/customer/TrxID), gateway as a select of configured gateways, pagination, CSV export, clickable rows → detail drawer (fees/net/timeline/refund history), distinct first-run vs no-results empty states, Retry on load error. | 3 | F3.3, F3.5, F3.7 — `transactions.astro` |
| 10 | **Visible merchant context + link lifecycle** — merchant switcher (or at least a merchant label) on every dashboard page; payment-link success panel with URL + native share actions; add deactivate/reactivate for links. | 1, 3 | F1.6, F1.7, F1.8 — `DashboardNav.astro`, `payment-links.astro` header comment |

### Quick wins (below the top 10, near-zero cost)

- Copy buttons for reference/TrxID on checkout instructions and `/pay` success (payment-links
  already has the `copyText()` pattern to reuse).
- Countdown ("Expires in 12:34") next to checkout `expires_at`.
- Hide the "Admin" nav link for non-admin users (`me()` already returns the user record).
- Auto-dismiss / timestamp the transactions page success banner; add Retry buttons beside
  every list-load error.
- Remove or auth-gate the payer-page FX selector — it can never render for anonymous payers.

---

*Audit only — no source files were modified. Companion audits in this folder cover visual /
design-system, dashboard UI, checkout UX, Android, motion, and accessibility perspectives;
where this report and the checkout-UX audit overlap (fixes 1, 5, 6, 7), this ranking reflects
cross-journey impact.*
