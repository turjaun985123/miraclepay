# Review 180 — Dashboard UI, Design Review #2

**Role:** Dashboard UI Engineer, MiraclePay design team (Wave 180)
**Date:** 2026-11-26
**Scope (read-only, no source edits):** `frontend/src/pages/dashboard/` (8 pages), `frontend/src/pages/admin/` (2 pages), `frontend/src/components/DashboardNav.astro`, shared `frontend/src/lib/ui.ts`, `frontend/src/layouts/BaseLayout.astro`, `frontend/src/components/{EmptyState,StatCard,PageHeader}.astro`.
**Baseline:** `design-audit/05-dashboard-ui.md` (Wave 151, file on disk is `05-dashboard-ui.md`, not `05-dashboard.md`).
**Method:** full structural extraction per page (badge imports/calls, `style=` counts, `<style>` blocks, `.num`, pagination, confirm/toast/field-error/skeleton usage, table headers), plus targeted reads. Counts below are reproducible with `grep -c 'style="'` / `<style` per file.

---

## 1. Verdict in one line

The Wave 151 shell and badge verdicts are **half-fixed**: a real sticky app shell with logout, identity and grouped nav now exists, and a single shared `statusBadge()` exists in `lib/ui.ts` — but only **2 of 10 pages import it**, so the badge contradiction the audit was written to kill still ships on the Overview, Webhooks, Payment Links, Gateways and both Admin pages; tables, pagination and settings are improved on exactly the pages that were already strongest (Transactions, Invoices) and untouched everywhere else.

---

## 2. Per-page evidence table

| Page | Lines | Inline `style="` | `<style>` blocks | Style-block lines | Shared `statusBadge` import? | Local / hand-rolled badge |
|---|---:|---:|---:|---:|---|---|
| `dashboard/index.astro` | 1009 | 0 | 1 | 13 | **No** | Yes — local fn L363 |
| `dashboard/transactions.astro` | 580 | 0 | 1 | 14 | **Yes** L129 | No |
| `dashboard/payment-links.astro` | 619 | 0 | 1 | 56 | **No** | Yes — inline if/else L474–484 |
| `dashboard/invoices.astro` | 762 | 0 | 1 | 80 | **Yes** L277 | No |
| `dashboard/gateways.astro` | 615 | 0 | 1 | 66 | **No** (n/a, state badges) | Yes — class strings L413/419/426 |
| `dashboard/webhooks.astro` | 766 | **9** | 1 | 20 | **No** | Yes — local fn L451, **shadows shared name/signature** |
| `dashboard/api-keys.astro` | 659 | 0 | 1 | 94 | **No** | Local var named `statusBadge` L445 + own map L451–455 |
| `dashboard/settings.astro` | 397 | 0 | **0** | **0** | n/a (no badges) | — |
| `admin/index.astro` | 827 | **6** | 1 | 11 | **No** | Yes — `badgeClass` L424–425 |
| `admin/plugins.astro` | 518 | 0 | 1 | 25 | **No** | Yes — class strings L188, L433 |
| `components/DashboardNav.astro` | 112 | 0 | 1 | 20 | n/a | — |
| **Total (10 pages)** | **6752** | **15** | **9** | **341** | **2 / 8 badge pages** | **6 pages** |

Notes on the counts: Wave 151 reported Overview alone at 22 inline styles; it is now **0** — the `BaseLayout` utility classes (`.u-*`, `.w-*`, `BaseLayout.astro` "Utilities (Wave 175/199)") absorbed them. The remaining inline styles are concentrated, not diffuse: Webhooks 9 (L35, 38, 57, 99, 134, 135, 161, 162, 190 — mostly `margin:0` / `justify-content:space-between` that `.u-m0`/`.u-row-between` already cover) and Admin 6 (L82, 83, 249 in markup; L599–601 inside a JS `innerHTML` chart template, so not fixable by a utility class alone). JS style mutation is a second, uncounted channel: e.g. `admin/plugins.astro:429` `head.style.justifyContent`, `DashboardNav` script toggling `style.display`.

---

## 3. Passes — what Wave 151 asked for and got

1. **App shell exists (05 §1.1, §4.1 — substantially PASS).** `DashboardNav.astro:2–6, 41–65`: sticky `<header class="app-shell">` with wordmark, account email (`#mp-user-email`, populated from `me()`), language toggle moved into the shell bar, and a working **Log out** button wired to `api.ts logout()` → `/login` (script L84–107). The 151 finding "no logout anywhere, no identity" is closed.
2. **Nav is grouped and Admin is gated (05 §1.2 — PASS, with one regression in §4.4).** Links carry `group: Operate | Configure | Admin` (L15–26), group labels are rendered, and `adminOnly` items start `hidden` and are only unhidden when `me().is_admin` (L59, L98–100). Non-admins no longer see a link that 403s.
3. **Shared badge taxonomy exists and is better than the 151 spec (05 §2.1, §4.3.1 — PASS as a library).** `lib/ui.ts:9–41`: one `BADGES` map, humanised labels (never raw enum except unknown fallback), `refunded`/`partially_refunded` → new `.badge-info` (not `failed`, not neutral — the exact 151 contradiction resolved), `overdue → badge-failed`, `sent → badge-info`. `BaseLayout` gained `.badge-info` to match.
4. **Money alignment primitive exists and is used correctly where used (05 §2.0 — partial PASS).** `BaseLayout`: `.table .num { text-align:right; font-variant-numeric: tabular-nums }`, `.stat-value` is tabular. Transactions applies it end-to-end (header `transactions.astro:86`, cell `textCell(..., 'num')` in `renderTransactions`); Invoices likewise (header L172, `totalCell.className='num'` L~590).
5. **Pagination exists on the model page (05 §2.0 — partial PASS).** Transactions: `PAGE_SIZE=25`, `limit/offset` wired to the API, Prev/Next + page label (`transactions.astro:96–100, 434–455`). This is the 151 §4.6 step-2 deliverable, on one table.
6. **Native `window.confirm()` is gone from API Keys (05 §3.2 — PASS in intent, see §4.6 for the defect).** Revoke now goes through the shared `confirmAction` `<dialog>` helper with consequence copy ("…will stop working immediately. This cannot be undone.") and verb-labelled button (`api-keys.astro:509–520`).
7. **Settings is the cleanest page in the set (new pass).** Only page with **0 inline styles and 0 `<style>` blocks**; it consumes utilities (`.u-row-between`, `.u-m0`, `.w-sm/.w-md/.w-lg`) and the shared field system throughout, and keeps the merchant switcher. It is the reference for what "converted" should mean.
8. **Gateway config panel now scrolls into view (05 §2.2 Gateways — PASS).** `gateways.astro:508` `panelEl.scrollIntoView({behavior:"smooth"})` on selection; the 151 "click appears to do nothing" defect is closed. (It hand-rolls smooth scroll instead of the shared reduced-motion-aware `scrollToEl` in `lib/ui.ts` — minor.)
9. **Kept strengths still present:** Transactions refund panel-row, API Keys one-time secret reveal, Invoices live totals (`aria-live`, `invoices.astro:527–544`), Gateways write-only secrets, honest empty/error states, `aria-current="page"` in the nav.

---

## 4. Remaining inconsistencies — ranked fixes

### P0 — 1. Shared `statusBadge` adoption is 2/10; three pages still contradict the taxonomy

Only `transactions.astro:129` and `invoices.astro:277` do `import { statusBadge } from '../../lib/ui'`. Every other badge page re-implements it, and the re-implementations still disagree with the shared map on the same statuses 151 flagged:

- `dashboard/index.astro:363–380` — local `statusBadge()`: `refunded` and `cancelled` → `badge-failed` (L374). Shared map: `refunded → badge-info`, `cancelled → badge` (neutral). **Same transaction shows red on Overview, blue/info on Transactions.** It also prints the raw enum as the label (`badge.textContent = status`, L378) vs. shared humanised labels.
- `dashboard/webhooks.astro:451–480` — local `statusBadge()` with a **different signature** (returns `HTMLSpanElement`, shared returns an HTML string). Any future import is a silent break. Its map knows `dead_letter` (shared map doesn't — shared would fall back to neutral "dead letter") and renders raw `status` for everything else (L476–479).
- `dashboard/payment-links.astro:474–484` — hand-rolled if/else: `expired → badge-failed` happens to agree, but `inactive/disabled/deactivated/closed` are all collapsed to `badge-failed` with a capitalised raw string as label; shared map has `disabled → badge` (neutral).
- `dashboard/api-keys.astro:445–460` — a local **variable** named `statusBadge` shadows the shared name, fed by a page-local `presenceLabels` map that contradicts the shared taxonomy: `revoked → badge-failed` (shared: neutral/`badge`), `offline → badge-failed` (shared spec in 05 §4.3.1: neutral), `never_connected → badge-pending`.
- `admin/index.astro:424–429` — `badgeClass` is role, not status (admin = success), and merchant status is rendered as a raw `<select>` of unhumanised strings (L439–475) with no badge at all.
- `admin/plugins.astro:188, 433` and `dashboard/gateways.astro:413–429` — string-literal badge classes. These two are semantically defensible (Enabled/Disabled, Configured/Sandbox are state, not status), but `gateways.astro:419` renders **Disabled as `badge-pending`** (amber) while `plugins.astro:433` renders Disabled as neutral `badge` — the same word, two colours, two pages.

**Fix:** delete the local functions in `index.astro`/`webhooks.astro` (adapt call sites: shared returns a string → `innerHTML` / `insertAdjacentHTML`), route Payment Links and API Keys device-presence through an extended shared map (add `dead_letter`, `device_*` entries to `lib/ui.ts`), and add a lint/grep gate: no `badge-` class literal outside `lib/ui.ts` and `BaseLayout`.

### P0 — 2. `confirmAction` call in API Keys does not match the shared helper's signature

`lib/ui.ts:115`: `confirmAction(message: string, confirmLabel = "Confirm")`. `api-keys.astro:512–517` calls it with an **object** (`{title, body, confirmLabel, danger}`). As written the dialog paragraph receives the object (rendered as `[object Object]` via `textContent`) and the title/body/danger styling never applies — the Review-165 fix is present in form but broken in contract. **Fix:** either upgrade `lib/ui.ts` `confirmAction` to the object API (preferred — the call site's copy is better) or downgrade the call site; add one type-checked usage — the dynamic `await import()` at L510 is what let this slip past the compiler.

### P1 — 3. Tables: `.num` on 2 pages, no compact density, no sort, no sticky header

- `.num` appears in exactly **2 of 8** table pages (Transactions, Invoices). Admin's By-gateway table (Transactions/Completed/Volume headers, `admin/index.astro:85–95`), Admin stats-adjacent lists, Payment Links Amount/Uses, Webhooks Attempts/Last-status-code and API Keys dates all remain left-aligned proportional figures — the 151 "single most pervasive defect" is fixed only where it was already best.
- **Density:** still one density product-wide. No `compact` variant exists anywhere (`grep compact|density` across pages/components/layout/tokens = 0). BaseLayout cell padding is unchanged comfortable-only. The audit log, webhook deliveries and admin lists — the three tables 151 named for compact — are the least converted pages.
- **Sorting / sticky / export:** `aria-sort` / sortable = 0 hits on all 10 pages; no `position:sticky` thead; CSV export = 0 (Webhooks L21–22 hits are copy text, not export). Unchanged from 151.
- **Pagination:** Transactions only. Admin audit log still hard-caps with copy admitting it — "The 50 most recent recorded actions" (`admin/index.astro:245–248`) — with no way to page; Invoices, Payment Links, API Keys, Webhooks Events/Deliveries, Admin Users/Merchants/KYC all still render the full result set. No page puts page state in the URL.
- **Admin PII unchanged:** KYC queue still renders full `profile.nid_no` in the list (`admin/index.astro:642`, header L110), no masking, no detail view. No `<dialog>`/drawer detail view exists on any dashboard/admin page (only the shared confirm dialog).

**Fix order:** apply `.num` to the five remaining numeric columns (pure markup, one pass) → add `.table-compact` to BaseLayout and apply to audit/deliveries → paginate audit log (API already supports `page/per_page`) → mask NID to last 4 in list.

### P1 — 4. Nav: two sources of truth persist, and Plugins moved into the wrong group

- Explicit `active` prop vs. path derivation — the exact split 151 flagged — is unchanged in ratio: 5 pages pass it (`settings`, `webhooks`, `api-keys`, `admin/index`, `admin/plugins`), 5 derive it. `DashboardNav.astro:34–38` still prefers the prop, so a renamed route silently mis-highlights on the explicit pages.
- **Regression vs. 151 §1.2:** Plugins is now a first-class nav item but placed in **Configure** (`DashboardNav.astro:23`, `group:"Configure"`, not `adminOnly`) while it lives under `/admin/plugins` and requires admin. So the merchant nav now advertises an admin page to everyone — the inverse of the Admin gating fixed in the same file — and `admin/plugins.astro` still claims `active="admin"`, highlighting a different item than the one clicked.
- Still no sidebar, no environment (Live/Sandbox) indicator in the shell, no global search, no breadcrumb: the shell is a sticky top bar + grouped pill strip (wrapping, not scrolling, on desktop; `overflow-x:auto` only ≤640px, L106–111). Merchant switcher is still **not** in the shell — it remains page-local on Overview/Settings and merely name-interpolated elsewhere (Webhooks L354–394 reads `mp_merchant` with no switcher), so the 151 data-trust defect (viewing Merchant B believing it's A) is only mitigated on 2 pages.
- Active state is still tint-only (`--color-brand-soft` bg, L103) — no weight/indicator change — though contrast tokens have improved since 151.

**Fix:** drop the `active` prop everywhere (derivation only), move Plugins under `group:"Admin"` + `adminOnly:true` (or give it `active="plugins"` and keep it in Configure deliberately — pick one), move the merchant switcher into `DashboardNav`'s shell bar.

### P1 — 5. Settings cleanup is structural only — the 151 traps are all still there

Settings is style-clean (§3.7) but functionally unconverted: one `<form id="settings-form">` spanning three cards with a single non-sticky Save at the bottom (`settings.astro:46, 129–134`); no dirty state, no sticky save bar; **"Settle threshold (minor units)" still asks for poisha** (L122–125, hint "10000 = 100.00") — the 100×-error trap from 151 §3.4, verbatim; notifications/auto-settle are still native checkboxes (L86, 91, 117), no switch component; KYC and 2FA still live only on Overview (`index.astro:228–286`), not under Settings; webhook URL is still editable in two places. Success is still a persistent `.alert-success` banner (L130), not the shared toast.

**Fix:** major-unit threshold field with conversion in code, sticky dirty save bar, move KYC/2FA cards to Settings (Overview keeps summary + link).

### P2 — 6. Shared UI helpers other than badges are effectively unused

`lib/ui.ts` exports `toast`, `withBusy`, `skeletonAfter`, `copyText`, `fieldError` — across all 10 pages: `toast(` = 0, `withBusy` = 0, `skeletonAfter`/skeleton = 0, `fieldError` = 0, and Payment Links defines its **own** local `copyText` (`payment-links.astro:378`) instead of importing the shared one. Loading is still text strings ("Loading settings…", "Loading webhooks…"), errors still page-level `.alert-error` banners with no field-level errors or `aria-invalid` wiring in page code, success feedback still five different idioms. The library half of the design system landed; the adoption half didn't.

### P2 — 7. Local `<style>` blocks: legitimate components vs. duplicated patterns

341 style-block lines remain. Justified (genuinely page-specific): Gateways card grid (66), Plugins card (25), Transactions filter row (14), Overview result grid (13). Duplicated and ready to promote: `.form-grid` is defined independently in both `invoices.astro` and `payment-links.astro`; `.list-header` likewise in both; `.check-row`/`.check` in Gateways and Payment Links; `.visually-hidden` redefined in `api-keys.astro` although BaseLayout already provides it; `webhooks.astro` defines `.sr-only` — a second name for the same BaseLayout `.visually-hidden`; `admin/index.astro`'s block overrides shared components (`.stat-value`, `select.input` width/padding) — page-level overrides of the system are the drift mechanism 151 warned about. API Keys has the largest block (94 lines) driven by the scope-grid, which 151 already nominated as a shared component.

### P2 — 8. Density and page-length: composition unchanged

Create-form-above-list still governs Payment Links (create L42 → list L138), Invoices (L44 → L149) and API Keys (L98 → L146); no create-in-drawer/modal, no collapse. Overview is still 8 sections / 1009 lines (merchant-create, stats, onboarding, search, chart, create-payment, recent transactions, KYC, 2FA); Admin still 7 sections / 827 lines with mixed time scopes (all-time stat cards above "last 30 days" analytics, cards still unlabelled); Webhooks still puts the 12-row static event catalogue (L96) above the operational Events/Deliveries tables. Stat cards are still hand-written `.card.stat` markup on Overview and Admin — `StatCard.astro`, `EmptyState.astro` and `PageHeader.astro` components exist and are used by **0 of 10** pages (every page hand-rolls its `<h1>` header; grep for all three components = 0 hits).

### P3 — 9. Smaller leftovers

- Dates: per-page `toLocaleString()` helpers persist (`admin/index.astro:340`, `index.astro:385`, `transactions.astro:222`, `webhooks.astro:417`, `api-keys.astro:357`); Invoices is the exception with an explicit format (L554–557). No shared date formatter in `lib/ui.ts`.
- Admin analytics bars are inline-styled JS templates (`admin/index.astro:599–601`) — now token-coloured (`--color-chart-1`, the 151 `--accent` blue bug is fixed) but still unlabelled, unexportable, and outside the utility system.
- Overview "Live" indicator still renders as `.badge-success` (`index.astro:132`) — a status badge meaning "polling", the misuse 151 noted.
- Transactions actions cell still renders a bare `—` for non-refundable rows; same pattern in Invoices (Send column).
- `settings.astro:62` — duplicate `class` attribute on the brand-colour input (`class="input mono"` + `class="w-sm"`); the second is dropped by HTML parsing, so the width utility silently never applies. Evidence the page's cleanliness is skin-deep in places.

---

## 5. Ranked fix list (for the build waves)

| # | Fix | Effort | Evidence |
|---|---|---|---|
| 1 | Adopt shared `statusBadge` on index/webhooks/payment-links/api-keys/admin/plugins; extend shared map (`dead_letter`, device presence); lint-ban page-local `badge-` literals | S | §4.1 |
| 2 | Reconcile `confirmAction` object-vs-string API; type-check the api-keys dynamic import | S | §4.2 |
| 3 | `.num` on Admin/Payment-Links/Webhooks/API-Keys numeric columns; mask KYC NID in list | S | §4.3 |
| 4 | Drop `active` prop (derivation only); fix Plugins group/gating mismatch | S | §4.4 |
| 5 | Settings: major-unit threshold, sticky dirty save bar, re-home KYC/2FA | M | §4.5 |
| 6 | `.table-compact` density + apply to audit/deliveries; paginate audit log | M | §4.3 |
| 7 | Adopt `toast`/`fieldError`/`skeletonAfter`/`copyText` or delete them from `lib/ui.ts`; remove local `copyText` clone | M | §4.6 |
| 8 | Promote `.form-grid`, `.list-header`, `.check-row` to BaseLayout; delete duplicate `.visually-hidden`/`.sr-only` | S | §4.7 |
| 9 | Use `PageHeader`/`StatCard`/`EmptyState` components or remove them; merchant switcher into shell | M | §4.4, §4.8 |

*End of review. No source files were modified; this report is the only output.*
