# 05 — Dashboard UI Audit

**Role:** Dashboard UI Engineer, 10-agent design engineering team (Wave 151 audit)
**Date:** 2026-11-26
**Scope inspected (read-only, no source edits):**

- `frontend/src/components/DashboardNav.astro` — note: the brief said `pages/DashboardNav.astro`; the component actually lives in `components/`
- `frontend/src/pages/dashboard/` — `index`, `transactions`, `payment-links`, `invoices`, `gateways`, `webhooks`, `api-keys`, `settings` (8 pages, 5,386 lines)
- `frontend/src/pages/admin/` — `index`, `plugins` (2 pages, 1,345 lines)
- `frontend/src/layouts/BaseLayout.astro` — the de-facto dashboard design system (`.card`, `.stat`, `.table`, `.badge`, `.btn`, `.input`, `.empty`, `.alert`)
- `frontend/src/lib/api.ts` — data shapes and capabilities the UI does / does not use (`ListTransactionsParams.limit/offset/page`, `ListAuditParams.page/per_page`, `MerchantAnalytics.daily[]`, `formatMoney()`)

**Method:** full read of `index.astro`, `transactions.astro`, `DashboardNav.astro`, `BaseLayout.astro`, `api.ts`; structural extraction (headings, `<th>` sets, forms, badges, empty states) plus targeted reads of the remaining eight pages.

**Verdict in one line:** a competent, accessible, single-column prototype — honest empty/error states, safe DOM rendering, real `aria-current` — that is not yet a *dashboard product*: there is no app shell, no pagination, no sorting, no right-aligned money, no detail view, and three pages re-implement the same status badge three different ways.

---

## 1. Shell / Navigation Critique

### 1.1 There is no app shell — only a pill bar floating above a page

Every dashboard/admin page is the same two siblings:

```astro
<DashboardNav />
<main id="main" class="container stack">…</main>
```

Consequences, all verifiable in the markup:

| Missing shell element | Evidence | Cost |
|---|---|---|
| No global header | `BaseLayout` renders only `<slot />`; no header exists in any page | No product identity, no account identity, nowhere for global actions to live |
| **No logout anywhere** | `logout()` exists in `api.ts` (Wave 25) and is called by **no** dashboard page; grep for logout/sign-out in `dashboard/` + `components/` = 0 hits | A payments dashboard you cannot sign out of |
| No user / merchant identity block | `me()` is fetched only to read `totp_enabled` on the home page | User cannot confirm *who* they are signed in as |
| No environment indicator | Gateway forms have a per-gateway Sandbox checkbox; the shell never says "you are in sandbox/live" | Real-money ambiguity — the highest-stakes shell omission for a payment product |
| Width mismatch | `<DashboardNav>` is a direct `<body>` child (full viewport width); `<main>` is `.container` (`max-width: 1120px`, centred) | The nav card's edges do not align with the content card edges below it on any viewport > 1120px |
| No footer / version / support link | — | Admin header has to state "MiraclePay v0.0.1" in body copy because the shell doesn't |

### 1.2 Information architecture

- **Flat, 9 items, one level, no grouping:** Overview · Transactions · Payment Links · Invoices · Gateways · Webhooks · API Keys · Settings · Admin (`DashboardNav.astro`, `links[]`). These are three different jobs forced into one row:
  - *Operate* — Overview, Transactions, Payment Links, Invoices
  - *Configure* — Gateways, Webhooks, API Keys, Settings
  - *Platform* — Admin
- **Admin does not belong in the merchant nav.** It is shown to every user and only fails after navigation (both admin pages render an "Admin access required" card on 403). Non-admins should never see the item; admins need a visibly separate zone, not the 9th pill.
- **Admin sub-navigation doesn't exist.** `/admin/plugins` is reachable only via a `Manage plugins` ghost button in the admin page header, and `plugins.astro` passes `active="admin"` — so two different pages claim the same nav state and there is no way to move Admin → Plugins except back through that button.
- **Orphaned functionality on Overview:** KYC submission, 2FA setup, global Search, Create Payment, onboarding checklist and the merchant creator all live *only* on `dashboard/index.astro` (988 lines, 5 forms, 7 sections). Settings (`settings.astro`) covers branding/notifications/payouts but not KYC or 2FA — a user looking for "security settings" under Settings will not find 2FA. IA rule violated: *account-level* concerns (2FA, KYC) are filed under a *merchant-overview* page.
- **Merchant context is invisible and inconsistent.** The merchant switcher (`#merchant-switcher`, a `<select>`) exists only on Overview and Settings. Transactions, Payment Links, Invoices, Gateways, Webhooks and API Keys silently operate on `localStorage['mp_merchant']` (falling back to the first merchant) with, at best, the merchant *name* interpolated into intro copy (Gateways, Invoices, Webhooks). A multi-merchant user can be looking at Merchant B's transactions while believing they are on Merchant A, with no on-screen evidence either way. This is a data-trust defect, not a cosmetic one.
- **No breadcrumbs, no page-level context beyond `<h1>` + one muted sentence.** Every page repeats the pattern `<h1>` + descriptive paragraph; none shows hierarchy (e.g. Admin › Plugins), parent links, or object context.

### 1.3 Active states

What works — keep it:

- Real semantics: `aria-label="Dashboard"` on the `<nav>`, `aria-current="page"` on the active link, pill highlight via `.is-active` (`background: var(--brand-soft); color: var(--brand-dark)`).
- Sensible fallback: when `active` is omitted, it is derived from `Astro.url.pathname` (`derivedActive`), so the five pages that render `<DashboardNav />` with no props still highlight correctly.

What doesn't:

- **Two sources of truth.** Five pages pass no prop, five pass it explicitly (`settings`, `webhooks`, `api-keys`, `admin`, `admin/plugins`). If a route is renamed, the explicit-prop pages silently highlight the wrong item; the derived pages self-heal. Pick derivation only, delete the prop, or assert it in review.
- Active vs. hover is low-contrast differentiation: hover = `background: var(--bg)` (near-white grey), active = pale green tint. At a glance in a scrollable row, "where am I" depends on a subtle tint difference, with no weight, underline, dot, or icon change.
- The language toggle (`#mp-lang-toggle`, "বাংলা") is styled as a nav link and sits as the last item in the same list — it reads as a 10th destination, not a utility control, and is the first item pushed off-screen on mobile (see §1.4).

### 1.4 Mobile behaviour

- The nav is a **horizontally scrolling pill strip**: `.dashboard-nav { overflow-x: auto }`, `.dashboard-nav-list { min-width: max-content }`. It scrolls rather than collapsing — acceptable as a stopgap, but: no scroll fade/shadow hint that more items exist, no snap, the active item is not scrolled into view on load, and the language toggle is at the far end of the scroll (effectively hidden on a 360px phone, where ~3½ pills are visible).
- There is **no hamburger, no bottom bar, no "More" menu** — discoverability of Invoices/Gateways/Webhooks/API Keys/Settings/Admin on mobile depends entirely on the user guessing the strip scrolls.
- Content: `.container` drops to `1rem` padding ≤640px; `.grid` (`auto-fit, minmax(220px,1fr)`) stacks stat cards 1-up — fine. Tables rely on `.table-wrap { overflow-x: auto }` — the 8-column Transactions table and 6-column admin KYC table become horizontal-scroll regions with no sticky first column, so the identifier column scrolls away from the status/action it identifies.
- Tap targets: nav pills are `0.375rem 0.75rem` padding at `0.875rem` type ≈ 32px tall — below the 44px touch guidance; `.btn-sm` row actions are similarly short.
- `color-scheme: light` is hard-coded in `BaseLayout` — no dark mode exists for the shell to support.

### 1.5 Shell recommendations (feeds §4.1)

Move to a real app shell: persistent left sidebar (grouped, collapsible) + 56px top bar carrying merchant switcher, environment badge, search and the user menu (with the missing Logout). Keep the pill bar only as the ≤1024px fallback, with the active item auto-scrolled into view.

---

## 2. Data Presentation Audit — Per Page

### 2.0 Cross-cutting table findings (the shared `.table` in `BaseLayout`)

| Property | Current state | Assessment |
|---|---|---|
| Density | One density only: `padding: 0.625rem 0.75rem`, `0.9375rem` body type; ≤640px `0.5rem` | Comfortable-only. Fine for 5 rows, wasteful for the 100+ row lists these pages will become; no compact variant |
| Alignment | **Everything left-aligned** (`.table th, .table td { text-align: left }`) | Money, counts and attempts are left-aligned on every page — amounts do not line up by decimal/unit and cannot be scanned or compared down a column. This is the single most pervasive data-presentation defect |
| Numerals | No `font-variant-numeric: tabular-nums` anywhere; amounts rendered via `formatMoney()` → `"BDT 1,234.56"` (code prefix, not suffix/symbol) | Proportional digits + left alignment = ragged money columns. Currency prefix repeated on every cell adds noise; it belongs in the column header (`Amount (BDT)`) |
| Dates | `new Date(x).toLocaleString()` re-implemented per page | Locale- and timezone-dependent, full date+time verbosity in a narrow first column, inconsistent across pages that copy the helper differently. No relative time ("2h ago") with absolute tooltip |
| Headers | Uppercase, `0.8125rem`, muted, `scope="col"` ✓; **not sortable, not sticky** | No `aria-sort`, no sort affordance at all — despite `api.ts` accepting filter params, no page offers ordering. Long tables lose their header on scroll (no `position: sticky` on `thead`) |
| Row interaction | `tbody tr:hover { background: var(--bg) }` only; rows are not clickable, no row selection, no checkbox column | No path from a row to a detail view (see "Detail views" below) |
| IDs | Transactions truncates to `id.slice(0,8)…` with full ID in `title`; other pages show full mono IDs or slugs | Truncation rule is page-local, not a spec; no copy-to-clipboard on any ID except payment-link URLs and new API keys |
| **Pagination** | **None. Anywhere.** No page renders pager, "load more", row count or page size | Critical gap. `api.ts` already supports it — `ListTransactionsParams { limit, offset, page }`, `ListAuditParams { page, per_page }` — and the UI uses none of it. Every list fetches and renders the entire result set; the admin audit log hard-caps at `limit: 50` with no way to see entry 51 |
| Sorting / filtering | Only Transactions has a filter UI (see §2.2). No page sorts | Invoices, links, keys, users, merchants, audit log: unfiltered, unsorted, unsearchable full lists |
| Export | No CSV/export on any list (Webhooks' only "download"-adjacent feature is signature testing) | Merchants cannot reconcile offline — table stakes for a payments dashboard |
| Loading | Text strings ("Loading transactions…", "Loading your dashboard…"), no skeletons, tables pop in | Layout shift on every load; the Overview additionally re-polls every 30s and re-renders, so the pop-in repeats |
| Empty states | `.empty { padding: 2.5rem 1rem; text-align: center; color: var(--muted) }` — plain centred text, no icon, no illustration, **no call to action** in the component itself | See per-page notes; the pattern is honest but inert |
| Detail views | **None.** No drawer, modal, dialog or detail route exists in any audited page (`<dialog>`/drawer/modal grep = 0) | The only expansions are: Transactions' inline refund panel-row, Gateways' config panel below the grid, Overview's create-payment result `<dl>`. Everything else about an object must fit in its table row |

### 2.1 Status badges — three implementations, three taxonomies

`BaseLayout` defines exactly three variants: `.badge-success`, `.badge-pending`, `.badge-failed` (plus neutral `.badge`). Every page then re-derives the mapping in its own script, and they disagree:

| Status string | Overview `statusBadge()` | Transactions `badgeClass()` | Result |
|---|---|---|---|
| `completed` | success | success | ✓ agree |
| `refunded` | **failed** (in the failed list) | **neutral** (no case) | ✗ same status, different colour |
| `cancelled` | failed | neutral | ✗ |
| `partially_refunded` | neutral | neutral, but row gets a Refund action | ✗ semantics unclear |
| `paid`, `approved`, `settled`, `successful` | success | neutral | ✗ |
| `initiated`, `created`, `awaiting_payment` | pending | neutral | ✗ |

Further badge problems:

- **Raw API strings are the label.** Overview prints `status` verbatim (`badge.textContent = status`); Transactions lowercases and replaces `_` with spaces; other pages differ again. The user sees `awaiting_payment`, `awaiting payment`, or the raw enum depending on which page they are on.
- No icon/dot, no shape difference — colour is the *only* channel distinguishing success/pending/failed (accessibility debt for colour-vision deficiency; the Accessibility seat should confirm).
- The taxonomy is too small for the domain: no `info` (refunded/neutral-terminal), no `sandbox/test`, no `revoked` treatment distinct from `failed`. API Keys already needs more — it renders status, scopes *and* Wave-36 `device_status` (`revoked | never_connected | online | idle | offline`) through the same three colours. Plugins collapses to Enabled (success) / Disabled (neutral). Webhook deliveries show status badge + attempts + last HTTP code with no badge for the HTTP outcome itself.
- Badges are also (mis)used as checklist markers on Overview's onboarding card (`Done` = success, `To do` = pending) and as count chips on Webhooks (`#events-count` is a `.badge` with no variant) — the component means three things.

### 2.2 Page-by-page

#### Overview — `dashboard/index.astro`

- **Stat cards:** 4 × `.card.stat` — Available / Pending / Total balance + "Recent transactions" (a *count of rows shown below*, labelled as if it were a metric). All four get identical visual weight, so the redundant Total (= Available + Pending, and its hint says so) competes with Available, the number users actually act on. No delta vs. previous period, no trend, no sparkline, no click-through (the balance cards go nowhere; there is no payouts page to go to). Loading state is the literal string `—` in a 1.625rem bold value. Currency comes from the balance payload, defaulting to BDT.
- **Chart:** see §2.3.
- **Recent transactions table:** 5 columns (Date, Reference, Gateway, Amount, Status) — a *different column set* from the Transactions page's 8 for the same entity (no ID, Customer, Provider Trx, Actions). It is not labelled as truncated/top-N and has no "View all" link — it renders whatever `listTransactions({merchant_id})` returns, so "Recent" is unbounded and the stat card counts it. Empty state is the best in the product: "No transactions yet for this merchant." Error is a separate inline alert ✓.
- Below the fold the page stops being a dashboard: Search card, Create Payment card, KYC card, 2FA card — four unrelated tools stacked in a single column (see §1.2, §3).

#### Transactions — `dashboard/transactions.astro`

- The strongest list page and the model the others should have followed: a dedicated filter card (Status `<select>` with All/Pending/Processing/Completed/Failed/Refunded + free-text Gateway + Filter/Reset), live re-filter on status change, a proper filtered empty state ("No transactions match these filters yet."), and the product's only inline row expansion (refund panel-row, §3.3).
- Columns (8): Date · ID (truncated) · Gateway · Amount · Customer (name · email · phone concatenated in one cell — un-scannable, and the widest column) · Provider Trx · Status · Actions.
- Defects: amount left-aligned; Customer should be name-primary with contact as secondary line; Refund is the only action and renders as a bare `—` for all other rows (a column of dashes); no date-range filter although `api.ts` supports `from`/`to`; no search by reference/ID/customer although the Overview search endpoint exists; no pagination/sort/export (§2.0); refund eligibility (`completed`, `partially_refunded`) is hard-coded in the renderer.

#### Payment Links — `dashboard/payment-links.astro`

- Create form permanently above the list (§3.1) pushes "Your payment links" below the fold.
- Columns (5): Title · Pay link · Amount · Uses (`n / max` or `n / Unlimited`) · Status. The pay-link cell with copy affordance is the page's best detail. Missing: Created date, expiry (the API/create form support `expires_at`/`max_uses` — expiry is collected but never displayed), and any per-link performance (views/payments beyond the Uses counter). No actions column — no deactivate/edit exists in the UI (the page comment states the API has no such call, and the UI honestly omits it rather than faking it ✓).

#### Invoices — `dashboard/invoices.astro`

- Columns (6): Number · Customer · Total · Status · Created · Actions (Send, for drafts). Total is **recomputed client-side from line items** (subtotal − discount, + tax in bps) rather than read from the invoice — a presentation-layer re-implementation of server money math that can disagree with the server's `amount_minor`.
- Missing: Due date (collected as due-days, stored as `due_at`, never shown — so "overdue" is unrepresentable and has no badge), customer contact, and any ageing/paid-vs-outstanding summary above the table. The "no hosted pay page in v0.0.1" notice is admirably honest but rendered as a permanent `.alert` banner above the list — it will be read 200 times by users who already know.
- The create form's live totals `<dl>` (Subtotal/Discount/Tax/Total, `aria-live="polite"`) is the best form-feedback pattern in the dashboard — adopt it elsewhere.

#### Gateways — `dashboard/gateways.astro`

- Not a table: a card grid (`repeat(auto-fill, minmax(230px,1fr))`) of all 14 gateways, each a full-card `<button>` with name, country badge and configured state; `.selected` gets a brand ring. This is the right presentation for a catalogue — and the only page that escapes the table monoculture.
- Defects: configured vs. unconfigured vs. enabled vs. sandbox are not four visually distinct states (only success/pending badges); a disabled-but-configured gateway and a never-configured one are hard to tell apart at grid distance. Clicking a card opens the config form *below the grid* — on a 14-card grid the form opens off-screen with no scroll-into-view or anchor, so the click appears to do nothing.

#### Webhooks — `dashboard/webhooks.astro`

- Four stacked sections compete: endpoint URL form → static 12-row event-catalogue reference table → Events table (Time · Type · Merchant · Event ID · Actions[Redeliver]) → Deliveries table (Event · URL · Status · Attempts · Last status code) → Test-signature tool.
- Events and Deliveries are the two tables an operator actually monitors, and both are below a read-only reference table. Neither has pagination, filtering by type/status, or auto-refresh (Overview polls; Webhooks — the operational page — does not, though it does have a manual Refresh button ✓). No payload viewer: an event row cannot be opened to see what was sent, and a failed delivery shows attempts + last code but no response body, no next-retry time, and no payload — debugging a failing webhook from this UI is impossible. Count badges on section headers (neutral `.badge`) are a good instinct, inconsistently applied elsewhere.
- The event catalogue (Event type · When it fires) is documentation living in a dashboard page; it belongs in docs with a link, not as the second section of an operational screen.

#### API Keys — `dashboard/api-keys.astro`

- Columns (7): Name · Key prefix · Scopes (rendered as mini-badges — the one place badges carry non-status meaning well) · Status · Last used · Created · Actions (Revoke).
- The one-time secret reveal is handled correctly as UX: a dedicated "Your new API key" card with a persistent (not toast) warning, Copy button and explicit Dismiss — the best destructive/secret pattern in the product; §4 generalises it.
- Defects: Revoke uses native `window.confirm()` — the only native confirm in the dashboard, unstyled, with no statement of consequence (existing integrations break immediately) and no undo; "Last used" shows raw timestamps with no "Never" state styling distinct from data (device_status `never_connected` exists in the API type and is not surfaced as a first-class state); no key-type (live/test) column; create form above list again (§3.1).

#### Settings — `dashboard/settings.astro`

See §3.2. No tables. Notable data point: this and Overview are the only pages with a merchant switcher — proof the pattern exists and simply was not propagated.

#### Admin — `admin/index.astro`

- **Stat cards:** 6 × `.card.stat` (Total transactions, Completed, Success rate, Gross volume, Total users, Total merchants) in the shared auto-fit grid — at 1120px they wrap 3+3 with no hierarchy; Success rate is derived client-side. No period selector; the stats are all-time while the analytics section below is "last 30 days" — two time scopes on one screen, neither labelled on the cards themselves.
- **Analytics:** see §2.3. The "By gateway" table (Gateway · Transactions · Completed · Volume (BDT)) hard-codes BDT in the header on a multi-currency platform, and has no success-rate or share column.
- **KYC review queue:** 6 columns (Merchant · Type · Trade licence · **NID** · Address · Decision). Full national ID numbers are rendered in a list table, in full, for every pending merchant — list views should mask PII (`••••1234`) with full values only in a deliberate detail view. Decision is an inline action with no detail view of the submission, no submitted-at date, and no note field in the row (the API accepts a decision `note`).
- **Users (Name · Email · Role · Joined)** and **Merchants (Name · Email · Currency · Joined · Status)**: read-only except merchant status, which changes "immediately" per the page's own warning — with no confirmation step for a suspend action. No search/filter on either, on the page most likely to have hundreds of rows. No pagination (§2.0) — the audit log below them is capped at 50.
- **Audit log:** Time · Actor · Action · Entity (+ filter form by action/entity). Actor renders as a raw user ID; action/entity as raw strings. No detail expansion for `details` (the payload that makes an audit log useful), no IP/UA columns although `AuditEntry` carries them.
- Page length: stats + analytics + KYC + users + merchants + platform settings + audit log = 7 major sections in one scroll, with no sub-nav or anchors (§1.2).

#### Admin Plugins — `admin/plugins.astro`

- Card-per-plugin layout (not a table) with Enabled/Disabled badge, per-plugin config fields and an inline Save with `Saving… / Saved.` status text — a nice micro-pattern. Toggle is a button that flips `enabled` immediately.
- Defects: no search (plugin lists grow), no kind/category grouping (gateway vs. SMS parser vs. transformer — kinds exist in the platform), config errors surface only as page-level alerts, and the page is unreachable from the nav (§1.2).

### 2.3 Analytics / chart presentation

Two charts exist. They share no code, no visual language, and no metric:

| | Overview "Sales — last 14 days" | Admin "Analytics — last 30 days" |
|---|---|---|
| Form | Hand-rolled inline SVG vertical bars, dependency-free | Stacked `<div>` rows with an inline horizontal bar (`innerHTML` template) |
| Metric | Completed **volume** (money) per day | Transaction **count** per day |
| Colour | Hard-coded `#0b7a55` (duplicates `--brand`) | `var(--accent, #2563eb)` — `--accent` **does not exist** in `BaseLayout`, so it silently falls back to a blue that appears nowhere else in the product |
| Axis / scale | No y-axis, no gridlines, no value labels; bar value only in an SVG `<title>` (hover tooltip) | No axis; width % of max, no value label specified in the bar itself |
| X labels | `day.date.slice(5)` at `font-size: 9` — sub-legible | Date text per row |
| Summary | Good: total volume + completed count + "Updated HH:MM:SS" text above the chart | None for the daily series |
| Live behaviour | 30s polling while tab visible, "Live · auto-refresh 30s" badge — the badge is `.badge-success`, i.e. "live" is rendered as a *status*, and every poll re-renders the whole dashboard (balance + transactions too), not just the chart | None |
| Empty / zero | All-zero days render as a flat baseline with no "no sales yet" message; a single max-normalised bar makes one small day look like a full-height success | `analytics-empty` ("No transactions yet.") shows only when no day has `count > 0` |
| Accessibility | `role="img"` + `aria-label` on the container ✓, but no data-table alternative — the numbers are unreachable to a screen reader beyond the total | Plain divs; values are at least in text |
| Range control | None — 14 days is hard-coded (`getMerchantAnalytics(id, 14)`); admin's 30 is separately hard-coded. No 7/30/90 selector, no comparison period, no gateway/method breakdown on the merchant chart | None |

Neither chart can be exported, neither handles currency other than assuming BDT in copy, and normalising each chart to its own max means bar heights are never comparable between visits.

---

## 3. Forms & Settings Patterns Audit

### 3.1 The dominant pattern — and its cost

Almost every page is **create-form card stacked directly above list card** (Payment Links, Invoices, API Keys, Overview's Create Payment, Gateways' grid→config panel). Consequences:

- The list — the thing returning users come for — is below the fold on every visit; the create form — used occasionally — is permanently expanded. No collapse, no "New" button → form, no modal/drawer.
- Long pages get longer: Overview stacks **seven** cards (stats, onboarding, search, chart, create payment, recent transactions, KYC, 2FA); Webhooks stacks five; Admin stacks seven sections.
- The pattern is at least *consistent* — same `.card`, `.field`, `.label`, `.input`, `.field-hint` from `BaseLayout` everywhere, which is why the dashboard feels coherent despite its length. Preserve the field system; change the page composition (§4).

### 3.2 Validation, errors, and feedback

- **Every form is `novalidate`** and validation is hand-rolled per page in JS. Quality varies: Overview's create-payment checks gateway + amount > 0 with clear messages; Invoices validates line items; Settings validates brand colour (`#rrggbb`) and logo URL client-side. There is no shared validator.
- **No field-level errors anywhere.** Every error — one bad field or a server failure — renders as a page- or form-level `.alert-error` banner (`#page-error`, `#form-error`, `#cm-error`, `#cp-error`, `#kyc-error`, `#twofa-error`, …). The user must re-read the whole form to find the problem; the offending field gets no `aria-invalid`, no red border, no inline message, and focus is not moved to it. Banners also vary in placement: top of page (Payment Links), inside the form (Invoices), below the fields (Settings) — no rule.
- **Required-ness is invisible.** `required` attributes exist in HTML but `novalidate` suppresses native UI, and labels carry no required marker or "(optional)" convention — except where copy happens to say so ("Trade licence no. (optional)" on KYC). Users discover required fields by submitting.
- **Success feedback is inconsistent:** banner alert (Payment Links, Settings "Settings saved."), inline status text (Plugins "Saved.", Webhooks' `#webhook-url-status`), result panel (Overview payment), full page reload (create merchant → `window.location.reload()`), or silent list refresh (Invoices send). No toast system exists; nothing auto-dismisses; success banners persist until the next action.
- **Submit protection is inconsistent:** most submit buttons disable during the request (`cm-submit`, `cp-submit`, settings submit) ✓, but the pattern is re-implemented per page; double-submit safety depends on each author remembering.
- **Destructive actions have three different standards:** API-key revoke = native `confirm()`; transaction refund = a well-built inline panel with amount + reason + explicit Confirm (the best pattern in the product — §3.3); merchant suspend (Admin) and plugin disable = **no confirmation at all**, immediate effect. Risk and friction are inversely matched: the irreversible platform action asks nothing, the recoverable one asks natively.

### 3.3 Patterns worth keeping (name them, then standardise)

1. **Inline expansion panel (Transactions refund):** expanding row, prefilled with the remaining refundable amount, optional reason, inline error `<span>`, Confirm/Cancel, single-panel-at-a-time, focus moved into the panel. This is the prototype for the detail drawer in §4.4.
2. **One-time secret reveal (API Keys):** persistent card, warning, copy button, explicit dismiss — never a toast, never auto-hide.
3. **Live totals (Invoices):** `aria-live` subtotal/discount/tax/total updating as line items change.
4. **Write-only secrets (Gateways):** secret fields are never prefilled; placeholder communicates "on file" without exposing the value. Correct and clearly hinted.
5. **Dynamic line items (Invoices)** and **scoped-checkbox grid with per-scope hints (API Keys)** — the two richest form controls in the product; both are page-local and should become shared components.

### 3.4 Settings specifically (`settings.astro` + settings-shaped sections elsewhere)

- **One form, three cards, one Save.** Branding / Notifications / Payouts are three `<section class="card">`s inside a single `<form id="settings-form">` with one "Save settings" button at the very bottom. No sticky save bar — after editing Branding at the top, the save control is two cards away, off-screen. No dirty state, no "unsaved changes" indicator, no per-section save, no reset-to-loaded.
- **Round-trip hazard, honestly handled:** the page loads the whole settings object and PUTs it back whole so unedited fields (`fee_bps`, …) survive — good engineering, invisible to the user, but it means a stale tab can silently overwrite a newer tab's changes; there is no conflict or "reloaded" affordance.
- **Units trap:** "Settle threshold (minor units)" asks merchants to type poisha (`10000 = 100.00`, explained in the hint). Everywhere else in the dashboard money is entered in major units and converted (`Math.round(x * 100)`). Settings is the one form that breaks the product's own money convention — a guaranteed source of 100× errors.
- **Good details:** brand colour has a synced colour-picker + hex text input; payout-day field appears conditionally for weekly/monthly with a rewritten label/hint; checkbox rows are label-clickable. But checkboxes/switches are unstyled native inputs everywhere (Settings, Gateways sandbox/enabled) — no switch component for what are conceptually toggles, and Gateway's two consequential toggles ("Sandbox", "Enabled") are visually identical checkboxes in one row.
- **Settings are fragmented across three homes:** merchant Settings (branding/notifications/payouts), Overview (KYC, 2FA), merchant-create form + Webhooks page (webhook URL — editable in *two* places: the create-merchant form field and the Webhooks endpoint form, with no indication they are the same value), Admin (platform name, default fee bps). No settings index, no search, no grouping by account vs. merchant vs. platform.
- **Admin platform settings** repeat the pattern at platform level: a flat form, one save, no change summary, no audit-trail link — despite an audit log existing on the same page.

### 3.5 Form mechanics (shared)

- Layout: single-column fields by default; ad-hoc grids via inline styles (KYC `grid-template-columns:1fr 1fr` inline, Invoices' page-local `.form-grid`, Overview's 22 inline `style=` attributes — the highest inline-style count of any page). No shared 2-column form-grid token, so field widths and gutters differ per page.
- Inputs are one height/radius from `BaseLayout` ✓; focus state is a brand border + soft outline ✓. No input adornments (no currency prefix inside amount fields — the currency is a separate `<select>` on Overview/Invoices but a free implication elsewhere), no masks for phone/NID/trx IDs, `inputmode` used well in places (decimal, numeric) but not uniformly.
- No autosave, no draft persistence, no multi-step wizards — the merchant-create and KYC forms are short enough to survive that; gateway configuration (per-gateway dynamic labels/hints for number vs. UPI VPA) is already straining the single static form.

---

## 4. Proposed Dashboard Design Patterns — Concrete Specs

Specs below are written to be implementable against the existing token names in `BaseLayout` (`--brand`, `--surface`, `--border`, `--radius`, …) plus the new tokens each spec introduces. Sizes are targets for the design-system wave, not descriptions of current code.

### 4.1 App shell

```
┌────────────┬──────────────────────────────────────────────┐
│ Sidebar    │ Top bar 56px                                  │
│ 240px      │ [Merchant ▾] [● Live]        [Search] [User ▾]│
│ (64px      ├──────────────────────────────────────────────┤
│ collapsed) │ Page header: H1 24px + description + actions  │
│            │ Content: max-width 1200px, padding 24px       │
└────────────┴──────────────────────────────────────────────┘
```

- **Sidebar:** `240px` expanded / `64px` icon-only collapsed (persist choice); background `--surface`, right border `1px --border`. Sections with `11px` uppercase group labels: **Operate** (Overview, Transactions, Payment Links, Invoices) · **Configure** (Gateways, Webhooks, API Keys) · **Account** (Settings, Verification/KYC, Security/2FA). **Admin** rendered only for `is_admin`, separated by a divider, with its own sub-items (Overview, Plugins, Audit log).
- **Nav item:** `36px` tall, `8px` radius, `14px/500` label + `18px` icon; hover `--bg`; active: `--brand-soft` background, `--brand-dark` text, `600` weight, `3px` left indicator bar. Count/alert badges right-aligned (e.g. KYC queue count in Admin).
- **Top bar:** `56px`, `--surface`, bottom border. Left: merchant switcher (see below). Centre-left: environment badge — `Live` = `.badge-success` with dot, `Sandbox` = `.badge-pending` with dot; global, always visible, reflects gateway mode. Right: global search (opens the existing `/api/v1/search` in a dropdown, `⌘K`), language toggle (moved out of nav), user menu (name, email, role; items: Settings, Security, **Log out** → `api.ts logout()`).
- **Merchant switcher (global, in top bar, on every page):** `<select>`-replacement dropdown showing merchant name + currency + status dot; switching re-scopes the page in place (no reload), persists to `mp_merchant`, and is announced via `aria-live`. Pages must never silently fall back to "first merchant" without showing which merchant is active.
- **Breakpoints:** ≥1280px sidebar 240px · 1024–1279px sidebar 64px · <1024px sidebar hidden, top bar gains menu button opening the sidebar as an overlay drawer; the current pill nav is retired (or kept <640px as a secondary in-page tab strip only where a page has sub-views). All interactive targets ≥ `44×44px` on touch.
- **Page header pattern:** H1 + one-line description left; primary action right (`New payment link`, `New invoice`, `New key`); optional breadcrumb above H1 on sub-pages (`Admin / Plugins`) at `13px` muted.

### 4.2 Stat cards

One component, three variants (plain / delta / sparkline). Anatomy and spec:

```
┌─────────────────────────────┐
│ AVAILABLE BALANCE      (i)  │  label: 12px, 600, uppercase, --muted, ls .04em
│ BDT 12,340.56               │  value: 28px/700, tabular-nums, currency code 14px muted prefix
│ ▲ 8.2% vs prev. 30 days     │  delta: 13px; ▲ --brand-dark / ▼ --danger, with period label
│ Ready to withdraw →         │  hint/action: 13px muted; whole card is a link if it has a target
└─────────────────────────────┘
```

- **Box:** `--surface`, `1px --border`, radius `12px`, padding `20px`, min-height `120px`; grid: `repeat(auto-fit, minmax(240px, 1fr))`, gap `16px`. Max 4 cards per row on Overview — choose the four by decision value (Available balance, Volume 30d, Success rate, Pending/failed needing attention); drop derived duplicates (Total = Available + Pending moves into the balance card as a secondary line: "Pending BDT X · settles by [date]").
- **Numbers:** `font-variant-numeric: tabular-nums` mandatory; format via `formatMoney()` but split code/value so the code can be de-emphasised. Zero is a real value (`BDT 0.00`), never `—`; `—` is reserved for "not loaded / not applicable" and must carry a tooltip or hint saying why.
- **Delta:** always present when a comparison exists, always labelled with its period ("vs previous 30 days"), never colour-only (arrow glyph + sign). No delta without a defined comparison — do not ship the current "count of rows shown below" pseudo-metric.
- **States:** loading = skeleton block (value bar `28px` tall, `--bg` shimmer, respects `prefers-reduced-motion`); error = card keeps last value, adds `13px` danger line "Couldn't refresh · Retry"; stale (after failed poll) = muted "Updated 4 min ago".
- **Admin variant:** same component; cards declare their time scope in the label ("Gross volume · all time" / "· last 30 days") so mixed scopes (§2.2 Admin) become explicit.

### 4.3 Table spec

A single `<DataTable>` pattern (Astro component + small client behaviour) replacing every hand-rolled `<table class="table">`:

| Element | Spec |
|---|---|
| Container | `--surface` card, radius `12px`, border `--border`; header row inside card (title + count + actions), table below a divider — not a separate card per table |
| Density | Default (comfortable): row `48px`, cell padding `12px 16px`, body `14px`. Compact variant: row `36px`, padding `8px 16px`, `13px` — used for audit log, webhook deliveries, admin lists. Density is a per-table prop, not a user toggle (v1) |
| Header | `12px/600` uppercase muted, `--bg` background, `position: sticky; top: 0` within the scroll region; sortable columns show ↕ on hover, ▲/▼ + `aria-sort` when active; sorting is server-side via existing API params where supported |
| Alignment | Text/ID/status: left. **Money, counts, attempts, rates: right**, header right-aligned to match. Dates: left, format `12 Nov 2026, 14:32` (absolute) with relative time in `title`; never raw `toLocaleString()` |
| Numerals | `tabular-nums` on all numeric/money/ID cells; IDs in `--font-mono` at `13px` |
| Money cells | Value only (`1,234.56`); currency stated once in the header (`Amount (BDT)`). Mixed-currency tables add a `13px` muted currency suffix per cell |
| Primary cell | First column is the identifier (reference / number / title / name) at `500` weight, `--ink`; secondary descriptor on a second line at `13px` muted (e.g. Customer: name / email below) — replaces concatenated `name · email · phone` cells |
| Status | Badge component per §4.3.1 — one mapping module shared by all pages |
| Row actions | Primary row action as text button; rest in a `⋯` menu (`32px` target, `44px` on touch). No `—` placeholders: ineligible rows render an empty actions cell. Whole row clickable → opens detail drawer (§4.4) when a detail view exists, with actions stop-propagated |
| Selection | Optional checkbox column (`16px` boxes, header = select-page) only where a bulk action exists; do not add speculatively |
| Pagination (mandatory) | Footer bar: left "Showing 1–25 of 342", centre/right: page-size select (25 / 50 / 100, default 25) + Prev / page numbers (current ±1, first, last, ellipsis) / Next. Server-side using `limit`+`offset` (transactions) or `page`+`per_page` (audit). Page state in the URL query (`?page=2`) so back/refresh/share work. "Load more" is not an acceptable substitute on operational tables |
| Empty states | Three distinct states, one component: **(a) Genuine empty** — icon (`48px`, muted), one-line explanation, primary CTA ("Create your first payment link") + secondary docs link. **(b) Filtered empty** — "No transactions match these filters", CTA = "Clear filters" (one click resets the filter bar). **(c) Error** — danger icon, error message, Retry button; never render error as an empty table or a page-level banner alone |
| Loading | Skeleton rows (5 × row-height bars) inside the table frame; header and filter bar stay mounted and interactive-disabled, so layout never shifts |
| Mobile | <720px: table → card list (primary cell + status badge on line 1; amount right-aligned; secondary fields as label/value pairs; actions in `⋯`). Horizontal scroll is the fallback only for genuinely wide operational tables (audit log), and then with a sticky first column + right-edge fade |
| Export | Where reconciliation is plausible (Transactions, Invoices, Admin lists): "Export CSV" ghost button in the table header, exporting the *current filtered* set, labelled as such |

#### 4.3.1 Status badge spec (single source of truth)

One `statusBadge(status)` in a shared module (superseding the three page-local versions in §2.1). Pill: radius `999px`, padding `2px 10px`, `12px/600`, `6px` dot in current colour before the label (colour is never the only signal), label = humanised map, never the raw enum.

| Group | Statuses | Variant | Treatment |
|---|---|---|---|
| Success (terminal) | `completed`, `paid`, `settled`, `successful`, `verified`, `approved` | `--brand-soft` bg, `--brand-dark` text | dot filled |
| In-flight | `pending`, `processing`, `initiated`, `created`, `awaiting_payment`, `draft`, `sent` | `--warning-soft` bg, `--warning` text | dot hollow |
| Failed (terminal) | `failed`, `failure`, `expired`, `rejected` | `--danger-soft` bg, `--danger` text | dot filled |
| Neutral (terminal) | `refunded`, `partially_refunded` ("Partially refunded"), `cancelled`, `revoked`, `disabled` | `--bg` bg, `--muted` text, `--border` border | dot hollow |
| Info / system | `online`, `sandbox`, `test` | new token needed: info-soft/info pair | dot filled |
| Unknown | anything else | neutral, label = raw string | log in dev; never crash |

`device_status` (API Keys) maps onto the same table (`online` → success, `idle` → pending, `offline` → neutral, `never_connected` → neutral "Never connected", `revoked` → neutral).

### 4.4 Filter bar + detail drawer

#### Filter bar (generalises the Transactions pattern — the only one that exists)

```
┌──────────────────────────────────────────────────────────────────┐
│ [Search: reference, customer, ID…      ] [Status ▾] [Gateway ▾]  │
│ [Date range: Last 30 days ▾]        [Reset]        [Export CSV]  │
└──────────────────────────────────────────────────────────────────┘
```

- **Placement:** sticky under the page header (`top: 56px` within the content scroll), inside the table card's header zone — not a separate card (Transactions' separate filter card wastes `~16px` + a card gap).
- **Controls:** search input first (debounced `300ms`, searches on type; placeholder names the searchable fields); facets as `<select>`s or dropdown-checkbox groups for multi-value (status); gateway becomes a select of configured gateways, not free text; date range as preset select (Today / 7d / 30d / 90d / Custom…) wired to the existing `from`/`to` API params.
- **Behaviour:** filters apply on change (no Filter button — Transactions' button + change-handler duplication goes away); active filters render as removable chips below the bar with a "Clear all"; filter state lives in the URL query alongside pagination, and changing any filter resets to page 1. Result count ("342 transactions") sits at the left of the bar and updates with the fetch.
- **Spec per page:** Transactions (search, status, gateway, date) · Invoices (search, status incl. Overdue — derived from `due_at`, date) · Payment Links (search, status) · Webhooks Events/Deliveries (type/status, date) · Admin Users/Merchants (search, role/status) · Audit log (actor, action, entity, date — replacing its bespoke form).

#### Detail drawer

The missing detail view (§2.0), standardised instead of inventing a page per entity:

- **Geometry:** right-side drawer, `480px` wide (max `92vw` on mobile, where it becomes full-screen), `--surface`, left border + shadow, slides in `200ms` (disabled under `prefers-reduced-motion`); overlay scrim `rgb(16 24 40 / .4)` on <1024px only (on desktop the table stays visible for context).
- **Behaviour:** opens on row click; updates the URL (`?tx=<id>`) so a detail is shareable and Back closes it; `Esc` and scrim click close; focus trapped while open, returned to the originating row on close; only one drawer at a time.
- **Anatomy:** header (entity title + status badge + close ×) → action bar (contextual primaries: Refund, Redeliver, Send, Revoke — using §3.2's confirmation rules) → definition-list body in labelled groups (`Amount`, `Customer`, `Gateway`, `Timeline`, `Metadata`) at `14px`, label `12px` uppercase muted, value tabular/mono as appropriate → raw-ID footer with copy buttons for every ID.
- **Content per entity (minimum):** Transaction — amount/fee/net breakdown, customer, provider trx + copy, refund history, metadata JSON (pretty, collapsible); Webhook event — full payload, delivery attempts timeline with HTTP code + response body per attempt; Invoice — line items + totals (server values), due/paid dates; API key — scopes, created/last-used, device status (secret never re-shown); KYC submission (Admin) — full NID etc., unmasked only here, decision note field + Approve/Reject; Audit entry — `details` payload, actor, IP, UA.
- **Refund flow migrates into the drawer** (from Transactions' panel-row, whose field design — prefilled remaining amount, reason, inline error — is retained verbatim inside the drawer).
- **Confirmation rule (applies everywhere):** destructive/irreversible actions (revoke key, suspend merchant, refund, reject KYC) use a modal confirmation *inside* the drawer stating the consequence in one sentence, with the action verb on the button ("Revoke key", not "OK"); native `window.confirm()` is banned; reversible toggles (plugin enable, gateway enabled) act immediately with an undo-able toast.

### 4.5 Forms & settings patterns (companion specs)

- **Create/edit in a drawer or modal, not a permanent card:** list pages get a primary "New …" button in the page header; the form opens in the §4.4 drawer (single-column forms) or a `640px` centred modal (Invoices' line-item form). On success: drawer closes, list refreshes in place, toast confirms ("Payment link created — [Copy link] [View]"). The create-above-list stack (§3.1) is retired on Payment Links, Invoices, API Keys, and Overview's Create Payment (which becomes a header action).
- **Field errors:** every field gets an error slot (`13px` danger text under the field, `aria-invalid="true"`, `aria-describedby` wiring); form-level banner reserved for server/network failures only; on submit error, focus moves to the first invalid field. Required fields marked with `*` + a one-time legend; optional fields unmarked (invert the current ambiguity in §3.2).
- **Settings layout:** left sub-nav (`200px`: Branding, Notifications, Payouts, Verification, Security, Webhooks) + one card per section in the content column — gathering the settings currently scattered across Settings/Overview/Webhooks (§3.4). **Sticky save bar** (`position: sticky; bottom: 0`, appears only when dirty) with "Unsaved changes" text, Save + Discard buttons; per-section save is acceptable if the bar is section-scoped — one global silent round-trip PUT is not. Money fields always major units with the currency as an input adornment; minor-unit values are converted in code, never asked of the user (fixes the settle-threshold trap).
- **Toggles:** a switch component (`40×24px`, brand when on) replaces checkboxes for binary settings with immediate or saved effect (notifications, auto-settle, sandbox, enabled); checkboxes remain for multi-select (API-key scopes).
- **Toasts:** bottom-right stack, `auto-dismiss 5s` for success, persistent for errors with actions; replaces the five banner idioms in §3.2. One-time secrets (§3.3.2) are explicitly *not* toasts — they stay in a modal until dismissed.

### 4.6 Implementation order (suggested for the build waves)

1. Shared `statusBadge()` + badge token additions (kills the §2.1 contradictions in one change).
2. Table spec: right-aligned numerics + `tabular-nums` + sticky header (pure `BaseLayout` CSS), then pagination on Transactions and Admin audit log using existing API params.
3. App shell (sidebar + top bar + global merchant switcher + logout).
4. Filter bar + URL state on Transactions; roll to Invoices/Links/Admin lists.
5. Detail drawer for Transactions and Webhook events; KYC masking in list + detail in drawer.
6. Stat-card component with deltas; chart consolidation onto one component (volume, labelled axes, data-table alternative, 7/30/90 range control).
7. Create-in-drawer migration; settings re-homing (KYC/2FA/webhook URL) + sticky save bar.

---

*End of audit. No source files were modified; this report is the only output.*
