# Review 195 — Design Systems (Wave 195, Design Review #3)

**Seat:** Design Systems Engineer · **Scope:** re-census of all 16 pages in `frontend/src/pages/` against Review 165 (`design-audit/review165-design-systems.md`): hex literals, inline `style="…"` attributes, page-local `<style>` lines, local badge/status re-implementations, `lib/ui.ts` helper adoption (`toast`/`withBusy`/`skeletonAfter`/`confirmAction`/`fieldError`/`statusBadge`, plus `copyText`/`scrollToEl`), and component adoption (`Icon`/`PageHeader`/`StatCard`/`EmptyState`/`DashboardNav`).
**Method:** `node scripts/check-tokens.mjs` run firsthand in `~/workspace/miraclepay/frontend` (output below), scripted per-page counts (inline = literal `style="` occurrences in source, including JS template literals; style lines = lines strictly between `<style>`/`</style>` tags — Review 165's per-page figures run uniformly +2/page vs this method, so deltas of −2 are method noise, not change), grep inventories of helper imports/call sites, local function definitions, class redefinitions, and token references. **Review only — no source files were edited.**

**Gate, firsthand:** `hex literals outside tokens.css: 0 (baseline 0)` — exit 0. ✅

## Verdict in one paragraph

Review 165's F-1 finally landed: inline styles collapsed **51 → 5** (dashboard/index 22→0, settings 11→0, webhooks 9→0, checkout/pay `float:right` gone), `window.confirm` is gone, checkout adopted the real `statusBadge`/`fieldError`/`copyText`, and merchant branding now sets `--color-brand` behind a real contrast gate (`brandAccent`, Wave 187). But the adoption program's core is still unadopted after three review cycles: `PageHeader`, `StatCard`, and `EmptyState` remain at **0 uses**, `toast`/`withBusy`/`skeletonAfter`/`scrollToEl` remain at **0 call sites**, 4 of 5 local status mappings survive, page-local `<style>` volume is flat, every §5 class collision from Review 165 is still live, and the one `confirmAction` adoption is **broken by a signature mismatch** (see C-1) — it will render `[object Object]` in the dialog. The gate also still checks hex only, so none of this drift can fail CI.

---

## 1. Census — per page (Wave 195, with Review 165 baseline)

Hex (gate) = `check-tokens.mjs` logic (link targets, `value=`/`placeholder=`, `#<digits>` copy stripped). Raw hex in parentheses where non-zero. Helpers = call sites in that page (calls to a page-local function of the same name are marked † and counted under "Local re-implementations", not as adoption).

| Page | Hex (gate) | Inline styles (165 → 195) | Local `<style>` lines (165 → 195) | ui.ts helpers used (call sites) | Components used | Local re-implementations |
|---|---:|---:|---:|---|---|---|
| `pages/index.astro` | 0 (raw 1, copy/anchor) | 0 → 0 | 42 → 40* | — none — | `Icon` ×3 | — |
| `pages/login.astro` | 0 | 0 → 0 | 0 → 0 | — | — | — |
| `pages/register.astro` | 0 | 0 → 0 | 0 → 0 | — | — | — |
| `pages/forgot-password.astro` | 0 | 1 → **0** ✅ | 0 → 0 | — | — | — |
| `pages/dashboard/index.astro` | 0 | 22 → **0** ✅ | 13 → 11* | `statusBadge` ×2† (own local fn) | `DashboardNav` ×1 | **Own `statusBadge()` at :363** |
| `pages/dashboard/settings.astro` | 0 (raw 2 = brand colour-picker `value`/`placeholder`, legitimate) | 11 → **0** ✅ | 0 → 0 | — none — | `DashboardNav` ×1 | — |
| `pages/dashboard/webhooks.astro` | 0 | 9 → **0** ✅ | 20 → 18* | `statusBadge` ×1† (own local fn) | `DashboardNav` ×1 | **Own `statusBadge()` at :451**; local `.sr-only` |
| `pages/dashboard/api-keys.astro` | 0 | 0 → 0 | 94 → 92* | `confirmAction` ×1 ⚠️ (broken, C-1) | `DashboardNav` ×1 | Local presence→badge map + element named `statusBadge` (:445–459); local `.visually-hidden` |
| `pages/dashboard/invoices.astro` | 0 | 0 → 0 | 80 → 78* | ✅ `statusBadge` ×1 (real import :277) | `DashboardNav` ×1 | Local `.form-grid`/`.list-header`/`.field` (scoped) |
| `pages/dashboard/payment-links.astro` | 0 | 0 → 0 | 56 → 54* | `copyText` ×1† (own local fn) | `DashboardNav` ×1 | **Own `copyText()` at :378** shadowing the helper; local `.form-grid`/`.list-header`/`.check-row` |
| `pages/dashboard/gateways.astro` | 0 | 0 → 0 | 66 → 64* | — none — | `DashboardNav` ×1 | Local `.check-row` (grid variant) |
| `pages/dashboard/transactions.astro` | 0 | 0 → 0 | 14 → 12* | ✅ `statusBadge` ×1 (real import :129) | `DashboardNav` ×1 | — (still the cleanest dashboard page) |
| `pages/admin/index.astro` | 0 | 6 → **5** (2 static + 3 in the JS chart-row template; the chart bar at :601 is dynamic/legitimate) | 11 → 9* | — none — | `DashboardNav` ×1 | **Inline `badgeClass` expression at :424**; local `.stat-value`, scoped `select.input` |
| `pages/admin/plugins.astro` | 0 | 0 → 0 | 25 → 23* | — none — | `DashboardNav` ×1 | 1 literal `font-size` |
| `pages/checkout/[id].astro` | 0 | 1 → **0** ✅ | 150 → 148* | ✅ `statusBadge` ×2, `fieldError` ×2, `clearFieldError` ×2, `copyText` ×1 (real import :384) + `brandAccent` | `Icon` ×1 | `.checkout-head .badge` scoped redefinition; `var(--p-white)` at :343 |
| `pages/pay/[slug].astro` | 0 | 1 → **0** ✅ | 61 → 59* | ✅ `fieldError` ×2, `clearFieldError` ×3, `copyText` ×2 (real import :221) + `brandAccent` | `Icon` ×1 | Literal font sizes ×3 (no local badge fn — status n/a on this page) |
| **Pages total** | **0** | **51 → 5** | **~632 → 608*** | see §2 | see §3 | **4 badge/status + 1 copyText** |

\* Style-line deltas are uniformly −2 on every styled page = the counting-method difference noted in Method, i.e. **page-local style volume is materially unchanged since Review 165** (Review 165's own stated total of 566 was inconsistent with its per-page sum of ~632; this review's method gives 608). No page's style block shrank or grew by more than method noise.

## 2. `lib/ui.ts` helper adoption (totals, 16 pages)

| Helper | Review 165 | Wave 195 | Where |
|---|---:|---:|---|
| `statusBadge()` | 2 real-import pages | **3 real-import pages** (+1) | ✅ `transactions` :129, `invoices` :277, **`checkout/[id]` :384 (new)** — checkout's local `badgeClass()` is deleted. Locals survive on dashboard/index, webhooks, api-keys (map), admin (inline expr) |
| `confirmAction()` | 0 | **1 — but broken (C-1)** | `api-keys` revoke, via dynamic `import()` at :510. `window.confirm` is **gone** from the codebase ✅ |
| `fieldError()` / `clearFieldError()` | 0 / 0 | **4 / 5 call sites, 2 pages** ✅ | `checkout/[id]` (×2/×2), `pay/[slug]` (×2/×3) — the two payer-facing forms only; no dashboard form uses them |
| `copyText()` | 0 (shadowed) | **3 real call sites, 2 pages** | ✅ `checkout` ×1, `pay` ×2. Still shadowed by payment-links' local `copyText()`; api-keys copy flow still doesn't use it |
| `toast()` | 0 | **0** ❌ | `#toast-region` still rendered by BaseLayout on every page, still never called (except internally by `copyText`) |
| `withBusy()` | 0 | **0** ❌ | Buttons still hand-juggle disabled/label state, incl. api-keys revoke right beside the new `confirmAction` call |
| `skeletonAfter()` | 0 | **0** ❌ | Bespoke `Loading …` paragraphs still on **12 pages** (all dashboard pages, checkout, pay, both admin pages) |
| `scrollToEl()` | 0 | **0** ❌ | — |
| `brandAccent()` / `contrastRatio()` / `luminanceOf()` (new since 165) | n/a | **2 pages** ✅ | `checkout` + `pay` gate merchant colour at ≥3:1 white-text contrast (Wave 187) — genuinely new, well-scoped adoption |

## 3. Component adoption

| Component | Review 165 | Wave 195 |
|---|---:|---:|
| `DashboardNav.astro` | 10 pages | 10 pages (all dashboard/admin) — unchanged, healthy |
| `Icon.astro` | 5 uses, landing only | 5 uses, **spread**: `index` ×3, `checkout` ×1, `pay` ×1 |
| `PageHeader.astro` | **0** | **0** ❌ — and its absence is no longer visible as inline styles (headers were rebuilt with global utility classes instead), so the component now competes with an entrenched hand-rolled pattern |
| `StatCard.astro` | **0** | **0** ❌ — dashboard/index and admin still hand-build `.stat` markup; admin still locally redefines `.stat-value` (:275) |
| `EmptyState.astro` | **0** | **0** ❌ — `.empty` markup still hand-written per page |

## 4. What closed since Review 165 (against its ranked fixes F-1…F-11)

| 165 fix | Status at 195 |
|---|---|
| F-1 Inline-style sweep / PageHeader | **Half-closed.** Inline 51 → 5 ✅ (target was ≤5 — met exactly, all 5 remaining on admin/index). But achieved *without* `PageHeader` (0 uses) — the drift is gone, the component is orphaned |
| F-2 Extend the gate beyond hex | ❌ **Not done.** `check-tokens.mjs` is still hex-only (header now reads "Waves 158/199"). The **17 literal `font-size:` values** in page `<style>` blocks are all still there (checkout ×6, api-keys ×4, pay ×3, gateways ×2, payment-links ×1, plugins ×1), as are the `--p-*` leaks |
| F-3 Replace 5 local status mappings | **1 of 5.** Checkout ✅. Dashboard/index, webhooks, api-keys, admin survive verbatim |
| F-4 `confirmAction` into api-keys revoke | ⚠️ **Nominally done, functionally broken — C-1.** `window.confirm` deleted, helper called — with the wrong argument shape (below) |
| F-5 Merchant branding per spec | **Mostly closed.** Both payer pages now `setProperty('--color-brand', accent)` (not just the `--brand` alias) and gate through `brandAccent()` ≥3:1 ✅. Remaining: no `color-mix` derivation — `--brand-dark`/`--brand-soft` in checkout/pay CSS still resolve to platform defaults while `--color-brand` changes; `submitButton.style.background = accent` direct write remains on both pages |
| F-6 Unify duplicated form blocks / kill collisions | ❌ **Not done.** Every collision from 165 §5 is still live: `.form-grid` ×2 + `.list-header` ×2 (invoices/payment-links, copy-pasted), `.check-row` ×2 with divergent models (gateways grid vs payment-links flex), `.visually-hidden` ×2 (global + api-keys :197), `.sr-only` (webhooks :256), admin `.stat-value`, checkout `.badge` scope |
| F-7 `toast`/`withBusy`/`copyText` adoption | **1 of 3.** `copyText` ✅ on payer pages. `toast` 0, `withBusy` 0 |
| F-8 `skeletonAfter` + `EmptyState` | ❌ **Not done.** Both at 0; `Loading …` pattern on 12 pages |
| F-9 Dark-mode hardening | ❌ **Not done.** D-1 still live: the `@media` dark block in `tokens.css` re-points `--shadow-sm/md/lg/xl`, the `:root[data-theme="dark"]` block still omits all four. D-3 partially moved: `--color-chart-1` is now actually used (dashboard chart via `getComputedStyle`, admin chart bar) but `--color-chart-2..5` remain unused and no chart token is re-pointed in either dark block. D-4 partially mitigated in `BaseLayout` (dark overrides flip `.btn-danger`/`.step-number` text to `--p-gray-950`) but `--p-white` is still consumed directly in `index.astro`, `BaseLayout.astro`, and `checkout/[id].astro:343`; no `--color-on-accent` exists. Still no `data-theme` writer anywhere |
| F-10 Retire legacy aliases | ❌ **Not done.** ~28 exact `var(--bg/--surface/--brand/--border/--muted/--shadow/--radius)` uses remain in pages/layout (same families as 165: `--border` ×10, `--brand` ×5, `--surface` ×4, `--muted` ×4, …); aliases remain defined in `tokens.css` |
| F-11 `StatCard` + `Icon` adoption | **Half.** `Icon` spread to payer pages ✅. `StatCard` still 0 |

## 5. New / critical finding

### C-1 — The sole `confirmAction` adoption calls it with the wrong signature (breaks at runtime)
`lib/ui.ts:115` declares `confirmAction(message: string, confirmLabel = "Confirm")` and renders `p.textContent = message`. `dashboard/api-keys.astro:510–518` calls it with a single **object** — `confirmAction({ title, body, confirmLabel: 'Revoke key', danger: true })`. The dialog will therefore show the literal text `[object Object]`, a default "Confirm" label, and no danger/title/body content — on the app's most destructive action (API-key revoke). Either the helper grows the object API the caller already expects (title/body/danger), or the caller is rewritten to the string API. Until one side moves, F-4 must be scored **not closed**. This is also the strongest argument for F-2: a type-aware or contract test would have caught what the hex gate cannot.

## 6. Ranked consistency fix list — Waves 196–199

| Rank | Fix | Wave | Gate / evidence of done |
|---|---|---|---|
| 1 | **Fix the `confirmAction` contract (C-1):** give `ui.ts` the object API (`{title, body, confirmLabel, danger}`) the api-keys caller already uses — or rewrite the caller to `(message, confirmLabel)` — and add a smoke/contract check that the revoke dialog renders its title/body. Then extend `confirmAction` to webhooks delete and admin suspend/payout | 196 | Grep shows one signature, caller matches it; dialog text verified; `window.confirm` stays at 0 |
| 2 | **Finish F-3: delete the 4 surviving local status mappings** — dashboard/index `statusBadge()` :363, webhooks `statusBadge()` :451, api-keys presence map (extend the shared `BADGES` table in `ui.ts` with the device-presence labels instead), admin inline `badgeClass` :424 — and import the helper on all four | 196–197 | `grep "function statusBadge" src/pages` = 0; `badgeClass` = 0; real-import pages 3 → 7 |
| 3 | **Extend `check-tokens.mjs` (F-2, third review asking):** fail on literal `font-size:` in page `<style>` blocks, `var(--p-*)` outside `tokens.css`/`BaseLayout`, local redefinitions of global classes (`.badge/.field/.input/.stat-value/.visually-hidden/.form-grid/.list-header/.check-row`), and local definitions of `statusBadge`/`copyText`/`toast` in pages. Then fix what it flags: 17 literal font sizes, `--p-white` ×3 files (add `--color-on-brand`/`--color-on-accent` and consume those) | 197 | Gate exits non-zero on a seeded violation; literal `font-size:` count = 0 |
| 4 | **Decide `PageHeader`/`StatCard`/`EmptyState`: adopt or delete.** Adopt `PageHeader` + `StatCard` on dashboard/index and admin (deleting admin's `.stat-value` redefinition), `EmptyState` on the list pages that hand-write `.empty`; any component still at 0 uses after this wave is deleted from `src/components/` so the library stops advertising dead patterns | 197–198 | Every component has ≥1 real consumer, or is gone; `.stat-value` definitions = 1 (global) |
| 5 | **Collapse the duplicated form blocks (F-6):** one global `.form-grid`/`.list-header`, one `.check-row` model (pick grid or flex, migrate the other page), delete api-keys `.visually-hidden` and webhooks `.sr-only` in favour of the global class | 198 | Each class in §4/F-6 has exactly one definition outside page scope overrides |
| 6 | **Adopt `toast()` + `withBusy()` + `skeletonAfter()` on the dashboard CRUD pages** (api-keys, payment-links — deleting its local `copyText()` — webhooks, settings, gateways, invoices): success feedback through the toast region that already ships on every page, busy state through the helper, `Loading …` paragraphs (12 pages) replaced by `skeletonAfter` while keeping the ids page scripts toggle | 198–199 | `toast(` ≥1 call site on ≥4 pages; `withBusy(` ≥1; `skeletonAfter(` ≥1; local `copyText` in payment-links = 0 |
| 7 | **Dark-mode + brand finish (F-9/F-5 residue):** deduplicate the two `tokens.css` dark blocks so shadows can't diverge (D-1); derive merchant `--brand-dark`/`--brand-soft` from the gated accent (or stop consuming them on payer pages) and drop the direct `submitButton.style.background` writes; re-point or delete `--color-chart-2..5`; record an explicit theme-toggle decision | 199 | One dark declaration source; `grep "submitButton.style.background"` = 0; chart tokens all used-and-dark-adjusted or all deleted |
| 8 | **Retire the legacy aliases (F-10):** migrate the ~28 remaining `var(--bg/--surface/--brand/--border/--muted/--shadow/--radius)` uses to semantic names and remove the alias block from `tokens.css`, closing the one-release bridge opened at Wave 152 | 199 | `grep "var(--border)"` etc. = 0 outside history; gate stays green |

*— Design Systems Engineer, Wave 195 review (Design Review #3). Companion: `review165-design-systems.md` (Review #1, baseline for all deltas above).*
