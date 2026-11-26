# Review 165 — Design Systems (Wave 165, Design Review #1)

**Seat:** Design Systems Engineer · **Scope:** `frontend/src/styles/tokens.css` vs Wave 151 spec (`design-audit/03-design-systems.md` §2), token consumption across all 16 pages, `frontend/src/lib/ui.ts` helper adoption, dark-mode risks, page-local `<style>` inventory.
**Method:** `node frontend/scripts/check-tokens.mjs` (run firsthand), full read of `tokens.css` and `lib/ui.ts`, scripted per-page counts of hex literals / `style="` attributes / `<style>` lines, a defined-vs-used scan of every `var(--…)` reference, and grep inventories of helper imports, local badge/status re-implementations, and class redefinitions. **Review only — no source files were edited.**

## Verdict in one paragraph

The token *foundation* landed almost exactly as spec'd and the colour-drift war is won: the gate passes at **0 hex literals outside `tokens.css`** (baseline 0), there are **zero undefined `var()` references** anywhere, and login/register/forgot-password now carry **no local styles at all**. But the *component/helper* half of the Wave 151 plan barely moved: `EmptyState`, `PageHeader`, and `StatCard` have **0 uses**, `toast`/`withBusy`/`skeletonAfter`/`confirmAction`/`fieldError` have **0 page call sites**, `statusBadge` is genuinely imported by only **2 pages** while at least 5 local re-implementations survive — including the exact `window.confirm` at `api-keys.astro:510` that Wave 151 Phase 4 named as the first thing to replace. Tokens are done; adoption is the remaining program.

---

## 1. `tokens.css` vs the Wave 151 spec (§2)

| Spec item (03 §2) | Implemented? | Evidence / drift |
|---|---|---|
| Three layers, one file, primitives only layer with hex | ✅ Yes | `tokens.css` (131 lines): `--p-*` primitives → `--color-*` semantics → legacy aliases; imported once in `BaseLayout.astro:7` |
| Palette values (green primary, violet demoted to accent, blue info-only, red/amber/grey ramps) | ✅ Yes, verbatim | All §2.2 primitive values match; `--color-accent` is violet, `--color-info` is blue |
| Single primary — login blue / register teal retired | ✅ Yes | login/register have 0 local style lines and use the global auth classes in `BaseLayout` |
| Dark semantic layer, `prefers-color-scheme` + `[data-theme="dark"]`, `color-scheme: light dark` | ⚠️ Mostly | Both blocks exist and `BaseLayout.astro:26` sets `light dark` — but the two dark blocks **diverge** (see §4, Risk D-1) and no code ever sets `data-theme` (grep = 0), so the explicit block is currently dead code |
| Back-compat aliases for all 16 legacy names, one release | ✅ Yes — now overdue | All present, plus `--accent` and `--brand-green` are now *defined* (fixing the two silent fallbacks found in Wave 151 §1.1). Still **~57 legacy `var()` uses** in pages/layout (e.g. `--border` ×10, `--brand` ×5, `--surface` ×4, `--muted` ×4, `--shadow` ×2); spec said remove in v0.0.4 — schedule the removal, don't let aliases become permanent |
| Type scale §2.3 (10 sizes, `--lh-*`, Bangla fallbacks in `--font-sans`) | ✅ Yes | All sizes present; `"Hind Siliguri", "Noto Sans Bengali"` appended; `:lang(bn)` rule added (Wave 156). Minor: per-size line-heights collapsed to just `--lh-tight`/`--lh-body` |
| Spacing scale §2.4 (4px base, `--space-*`, `--pad-card`, containers) | ✅ Yes | Complete scale; `--space-*` is now the most-used token family (115 `var()` uses) |
| Radius/shadow/z-index §2.5 | ✅ Yes | `--radius-*` (legacy `--radius` → `-lg`), `--shadow-xs…xl`, `--z-*` all present |
| Motion tokens §2.5 (`--duration-fast:150ms`, `--duration-med:250ms`, `--ease`) | ⚠️ Renamed, spec stale | Implemented as `--motion-instant/fast/base/slow` (75/120/200/320ms) + `--ease-out`/`--ease-in-out`. Consumers consistently use the new names, so this is a *documentation* drift — update `03`/DESIGN.md, don't rename code |
| `--color-border-strong` = `#d0d5dd` (spec §2.2) | ⚠️ Intentional deviation | Implemented as literal `#7d879c` with an in-file comment (≥3:1 on surface, WCAG 1.4.11 control boundaries). This is an improvement over spec, but it is a **hex in the semantic layer** and the spec document was never amended — record it in DESIGN.md |
| `--color-danger-text` | ➕ Addition, not in spec | `#b42318`-based, documented 6.05:1 on danger-soft; used by `.badge-failed`/`.alert-error`. Good addition; spec should absorb it |
| `--color-chart-1..5` + JS reads via `getComputedStyle` | ❌ Defined, never used | All five are in the "defined but never used" scan. Admin chart still uses inline `background:var(--color-info)` and dashboard chart JS sets `fill: currentColor`. Either wire the chart tokens or delete them |
| Merchant brand: override `--color-brand*` at page scope, hover/soft derived via `color-mix` | ❌ Not implemented as spec'd | `checkout/[id].astro:613` and `pay/[slug].astro:449` do `documentElement.style.setProperty('--brand', brandColor)` — the *legacy alias only* — plus a direct `submitButton.style.background =`. `--color-brand`, `--brand-dark`/hover and `--brand-soft` therefore stay default-green while the submit button turns merchant colour; no `color-mix` anywhere (grep = 0) |
| Breakpoints as named constants, px only | ✅ Yes | Header comment documents 480/640/900/1120; no `40rem` media spelling survives in the counts |

**Unused-token scan (defined in `tokens.css`, never referenced elsewhere):** `--color-chart-1..5`, `--color-overlay`, `--z-dropdown`, `--z-overlay`, `--shadow-xs`, `--radius-none`, `--radius-xs`, `--motion-instant`, `--motion-slow`, `--pad-card-lg`, `--space-0`, `--space-px`, `--z-base`, plus the `--accent`/`--brand-green` aliases (kept deliberately). `--color-overlay`/`--z-modal` being unused is consistent with §3 below: `confirmAction` builds a `<dialog>` but nothing calls it.

## 2. Drift table — per page (measured at Wave 165)

Hex = gate count from `check-tokens.mjs` logic (link targets, `value=`/`placeholder=`, and `#<digits>` copy stripped). Inline = `style="…"` attributes (template-literal chart bar in admin counted once). Style lines = lines inside page-local `<style>` blocks.

| Page | Hex (gate) | Inline styles | Local style lines | Notes |
|---|---:|---:|---:|---|
| `pages/index.astro` (landing) | 0 | 0 | 42 | Model consumer: 84 `var()` uses, `Icon` ×5; was 223 style lines / violet palette at Wave 151 |
| `pages/login.astro` | 0 | 0 | 0 | Fully global — blue palette gone |
| `pages/register.astro` | 0 | 0 | 0 | Fully global — teal palette gone |
| `pages/forgot-password.astro` | 0 | 1 | 0 | Inline style is token-based (`var(--color-border)`, `var(--space-6)`) — move to a utility class, trivial |
| `pages/dashboard/index.astro` | 0 | **22** | 13 | **Worst inline offender — unchanged from Wave 151 (22).** Zero `var()` uses in markup styles; also hand-rolls its own `statusBadge()` (line 363) |
| `pages/dashboard/settings.astro` | 0 | **11** | 0 | Still no style block, all layout via inline styles — unchanged from Wave 151. Raw hex ×2 in source are the brand colour-picker `value="#0b7a55"`/`placeholder` (legitimate runtime branding input, stripped by the gate) |
| `pages/dashboard/webhooks.astro` | 0 | 9 | 20 | Inline page-head rows ×3; local `.sr-only` duplicates global `.visually-hidden`; own local `statusBadge()` (line 451) |
| `pages/dashboard/api-keys.astro` | 0 | 0 | 94 | Largest dashboard style block; local `.visually-hidden` redefinition; status rendered via a local variable named `statusBadge` + local status map (lines 445–460), **not** the helper; `window.confirm` still at line 510 |
| `pages/dashboard/invoices.astro` | 0 | 0 | 80 | ✅ Real `statusBadge` import; but still redefines `.field`, `.form-grid`, `.list-header` locally |
| `pages/dashboard/payment-links.astro` | 0 | 0 | 56 | Defines its **own local `copyText()`** (line 378) shadowing the `lib/ui.ts` helper; `.form-grid`/`.list-header`/`.check-row` duplicated with invoices/gateways |
| `pages/dashboard/gateways.astro` | 0 | 0 | 66 | `.check-row` here is the grid variant — still divergent from payment-links' flex variant (Wave 151 §1.8, unresolved) |
| `pages/dashboard/transactions.astro` | 0 | 0 | 14 | ✅ Cleanest dashboard page: real `statusBadge` import, small filter-row block only |
| `pages/admin/index.astro` | 0 | 6 | 11 | 5 static inline styles + 1 legitimate dynamic chart bar; still redefines `.input` and `.stat-value` locally; inline `badgeClass` expression (line 424) instead of `statusBadge` |
| `pages/admin/plugins.astro` | 0 | 0 | 25 | Small, token-using; one literal `font-size: 0.8125rem` (= `--text-sm`) in its style block |
| `pages/checkout/[id].astro` | 0 | 1 | **150** | **Largest page-local block in the app.** Own `badgeClass()` (line 478) — the 5th status mapping; redefines `.badge`; inline style is legacy `float:right`; uses primitive `var(--p-white)` directly (line 320) |
| `pages/pay/[slug].astro` | 0 | 1 | 61 | `float:right` inline; literal font sizes in style block; merchant-brand issue in §1 applies here too |
| **Pages total** | **0** | **51** | **566** | Inline total is *identical* to Wave 151's 51 — the Phase 3 utility sweep never happened; it just moved (DashboardNav's inline reset was fixed, forgot-password gained one) |

Non-page files, for context: `layouts/BaseLayout.astro` — 159 style lines, 213 `var()` uses, 0 hex, 0 inline (the global sheet; healthy). `components/DashboardNav.astro` — 20 style lines, 38 `var()` uses, 0 inline (still the model component). `components/Icon/EmptyState/PageHeader/StatCard.astro` — 0 style lines each (pure markup over global classes).

**Literal values still hiding inside page `<style>` blocks** (the gate only checks hex, so these pass CI): literal `font-size` values in checkout (×6: `1.375/0.875/1/1.5rem`), api-keys (×4), gateways (×2), pay (×3), payment-links (×1), plugins (×1) — every one is exactly a `--text-*` token value written out longhand. The gate needs a second rule (see Fix F-2).

## 3. Component & helper usage (`lib/ui.ts`, components/)

| Asset | Status | Evidence |
|---|---|---|
| `DashboardNav.astro` | ✅ Adopted | Imported by all 10 dashboard/admin pages |
| `statusBadge()` (`lib/ui.ts:34`) | ⚠️ 2 of ~7 needed pages | Real imports: `transactions.astro:129`, `invoices.astro:277` only. Local re-implementations surviving: `dashboard/index.astro:363` (own function), `webhooks.astro:451` (own function), `api-keys.astro:445` (local map + element named `statusBadge`), `checkout/[id].astro:478` (`badgeClass()`), `admin/index.astro:424` (inline `badgeClass`). Wave 151 counted 5 duplicates; there are still 5 |
| `toast()` (`lib/ui.ts:40`) | ❌ 0 call sites | `#toast-region` is rendered by `BaseLayout:192` and `.toast` styles exist, but no page script ever calls `toast()` (grep of page scripts = 0). Success feedback still goes through page-level `.alert` divs |
| `withBusy()` | ❌ 0 call sites | No page imports it; buttons still hand-juggle labels/disabled state |
| `skeletonAfter()` | ❌ 0 call sites | The bespoke "Loading …" `<p>` pattern survives on ≥10 pages (`invoices`, `payment-links`, `settings`, `transactions`, `dashboard/index`, `checkout`, `pay`, `admin/index`, `admin/plugins`, …) with the same id-toggling scripts as Wave 151 |
| `confirmAction()` | ❌ 0 call sites | `window.confirm(...)` still at `api-keys.astro:510` — the single destructive-action guard Wave 151 Phase 4 said to replace *first*. The native-`<dialog>` helper, `.mp-dialog` styles and `--z-modal`/`--color-overlay` tokens are all built and all unused |
| `copyText()` | ❌ Shadowed, not used | `payment-links.astro:378` defines a local `copyText()` instead of importing the helper; api-keys' copy flow doesn't use it either |
| `fieldError()` / `clearFieldError()` | ❌ 0 call sites | Forms still hand-wire `aria-invalid`/error text per page |
| `scrollToEl()` | ❌ 0 call sites | — |
| `EmptyState.astro` | ❌ 0 uses | `.empty` markup still hand-written per page (the component is 9 lines and ready) |
| `PageHeader.astro` | ❌ 0 uses | Directly explains the 22+11+9 inline `justify-content: space-between` / `margin: 0` styles on dashboard/index, settings, webhooks — the component that kills them exists and is imported nowhere |
| `StatCard.astro` | ❌ 0 uses | dashboard/index and admin still hand-build `.stat` markup; admin still locally redefines `.stat-value` |
| `Icon.astro` | ⚠️ Landing only | 5 uses, all in `pages/index.astro`; dashboard pages use none |

**Pattern:** everything built in Waves 152–158 that required *editing a page's script* to adopt is unadopted; everything that only required *CSS* (tokens, global classes) is adopted. The remaining work is script-side integration, page by page.

## 4. Dark-mode correctness risks

| # | Risk | Severity | Evidence |
|---|---|---|---|
| D-1 | **The two dark blocks diverge on shadows.** The `@media (prefers-color-scheme: dark)` block re-points `--shadow-sm/md/lg/xl` to black-based values; the `:root[data-theme="dark"]` block omits all four. The moment a theme toggle sets `data-theme="dark"` on a light-OS machine, cards/toasts/dialogs get light-mode shadows on dark surfaces | Medium — latent (no toggle exists yet, grep `data-theme` in pages/lib = 0) | `tokens.css:96–113` vs `115–129` |
| D-2 | **Merchant brand colour breaks dark mode (and light hover/soft) at checkout/pay.** Only the `--brand` alias is overridden at runtime; `--color-brand` consumers, `--brand-dark` (hover) and `--brand-soft` keep default green, and a dark merchant colour that works on light `--color-surface` can be near-invisible on dark `#101828` — no `color-mix` derivation, no contrast guard, spec §2.2 risk (c) unaddressed | **High** — live today on every branded checkout | `checkout/[id].astro:611–614`, `pay/[slug].astro:448–450` |
| D-3 | **Chart colours have no dark story.** `--color-chart-1..5` are defined with light values only, never re-pointed in either dark block, and never used; the actual admin chart uses `--color-info` (which *is* dark-adjusted) and dashboard chart JS uses `currentColor`. If chart tokens get adopted as-is, dark charts inherit light-tuned colours | Medium | `tokens.css` dark blocks contain no `--color-chart-*`; §1 unused scan |
| D-4 | **Primitive leak:** `var(--p-white)` used directly in `BaseLayout.astro` (`.btn-danger`), `index.astro` (`.step-number`), `checkout/[id].astro:320`. White text is dark-safe in the first two, but `.step-number` puts white on `--color-accent`, whose dark value is the much lighter `--p-violet-300` — white-on-`#a78bfa` is a likely AA failure for that (non-interactive, large-ish) element; semantically these should be `var(--color-on-brand)` / a new `--color-on-accent` | Medium | grep `--p-` outside `tokens.css` = only `--p-white`, 3 files |
| D-5 | **No theme toggle + no forced-light escape for branded pages.** Dark is OS-only; checkout/pay inherit it automatically. Merchant logos/branding assets were presumably designed against light — unreviewed in dark (cannot be verified from source; needs the Wave 199 screenshot pass in both schemes) | Medium | No `data-theme` writer anywhere; `color-scheme: light dark` at `BaseLayout:26` |
| D-6 | Low-risk / healthy: `.toast`, `.skeleton`, `.mp-dialog`, badges and alerts are all built purely from semantic tokens (`--color-surface-raised`, `--color-surface-sunken`, soft/ink pairs), and both dark blocks agree on every `--color-*` value. The skeleton pulse and toast keyframes live in `BaseLayout` with the reduced-motion guard retained | — | `BaseLayout.astro:123–144` |

## 5. Remaining page-local `<style>` block inventory

Total page-local style lines: **566** (Wave 151: ~1,050 across pages; BaseLayout excluded both times) — real progress, concentrated in auth/landing. What remains, by kind:

| Block | Lines | Kind / disposition |
|---|---:|---|
| `checkout/[id].astro` | 150 | Checkout-specific layout (summary, instructions, submit) — mostly legitimate page scope, but redefines `.badge` and holds 6 literal font sizes; extract summary/definition-list patterns per Wave 151 component #22 |
| `dashboard/api-keys.astro` | 94 | Scope-grid/fieldset + new-key box — Wave 151 component #5/#20 (CheckboxGroup, CopyField) territory; also the duplicate `.visually-hidden` |
| `dashboard/invoices.astro` | 80 | `.form-grid`, `.list-header`, `.field`, items-fieldset — duplicates of payment-links/gateways blocks |
| `dashboard/gateways.astro` | 66 | Gateway cards + `.check-row` (grid variant) — component #5/#6 territory |
| `pay/[slug].astro` | 61 | Pay-page layout; shares shapes with checkout (brand line, amount, details) — one shared pattern would serve both |
| `dashboard/payment-links.astro` | 56 | `.form-grid`, `.list-header`, `.check-row` (flex variant) — exact duplicates of invoices/gateways |
| `index.astro` | 42 | Marketing-only classes (hero, features, pricing, verify card) — legitimate page scope; fully tokenised |
| `admin/plugins.astro` | 25 | Plugin card/output — legitimate; 1 literal font size |
| `dashboard/webhooks.astro` | 20 | `.url-cell` + duplicate `.sr-only` — delete the duplicate, keep the cell |
| `components/DashboardNav.astro` | 20 | Component-owned — correct place for styles |
| `dashboard/index.astro` | 13 | `.result-grid` only — its problem is inline styles, not this block |
| `dashboard/transactions.astro` | 14 | Filter-row classes — Wave 151 component #19 (FilterBar) territory |
| `admin/index.astro` | 11 | Redefinitions of `.input` + `.stat-value` — delete, don't keep |
| Zero-block pages | 0 | `login`, `register`, `forgot-password`, `dashboard/settings` — settings achieves it only by pushing everything inline (§2) |

**Class collisions still live** (Wave 151 §1.8, re-measured): `.field` ×2 definitions (global + invoices), `.input` ×2 (global + admin), `.stat-value` ×2 (global + admin), `.badge` ×2 (global + checkout), `.form-grid` ×2 and `.list-header` ×2 (invoices + payment-links, copy-pasted), `.check-row` ×2 with *different* layout models (gateways grid vs payment-links flex), `.visually-hidden` ×2 (global + api-keys) plus its alias `.sr-only` in webhooks.

## 6. Ranked fixes for Waves 166–180

| Rank | Fix | Waves | Why first / gate |
|---|---|---|---|
| F-1 | **Adopt `PageHeader` + kill inline styles on dashboard/index (22), settings (11), webhooks (9), admin (5 static).** Replace hand-built header rows with the existing component; add `.page-head`/spacing utilities only where the component can't reach | 166–167 | Biggest visible drift, zero design risk — the component already exists. Gate: inline count 51 → ≤5 (dynamic chart bar + merchant submit exempt) |
| F-2 | **Extend `check-tokens.mjs` beyond hex:** fail on literal `font-size:` values in page `<style>` blocks, on `var(--p-*)` outside `tokens.css`/`BaseLayout`, and on local redefinitions of `.btn/.field/.input/.badge/.card/.table/.alert/.visually-hidden`. Then fix the ~17 literal font sizes it flags | 166 | Today's gate passes while §2/§5 drift persists — the ratchet must cover the drift that actually remains |
| F-3 | **Replace all 5 local status mappings with `statusBadge()`** (dashboard/index, webhooks, api-keys, checkout `badgeClass`, admin inline) and delete the locals | 167–168 | Pure-JS, grep-verifiable, Wave 151 Phase 4's first item — still undone two review cycles later |
| F-4 | **Wire `confirmAction()` into api-keys revoke (kill `window.confirm` at :510), then webhooks delete and admin suspend/payout.** Add the missing `--color-overlay`/`--z-modal` usage this creates | 168–169 | Only destructive-action guard in the app is a browser default dialog; helper + tokens + styles are all built and idle |
| F-5 | **Fix merchant branding per spec §2.2:** set `--color-brand` (not just `--brand`) at checkout/pay scope, derive hover/soft with `color-mix` + static fallbacks, and add a contrast guard for dark surfaces (D-2) | 169–170 | Live correctness bug on revenue pages, in both colour schemes |
| F-6 | **Unify the duplicated form blocks:** one global `.form-grid`/`.list-header`, one `.check-row` model; delete admin's `.input`/`.stat-value`, api-keys' `.visually-hidden`, webhooks' `.sr-only`, checkout's `.badge` redefinition | 170–171 | Removes every remaining class collision in §5 |
| F-7 | **Adopt `toast()` + `withBusy()` + `copyText()`** on the copy/save/revoke actions in api-keys, payment-links (delete its local `copyText`), webhooks, settings, gateways; retire the page-level success alerts they replace | 171–173 | Feedback consistency; toast region/styles already ship on every page |
| F-8 | **Adopt `skeletonAfter()` + `EmptyState` page by page,** deleting the ≥10 bespoke "Loading …" paragraphs while keeping the element ids page scripts toggle | 173–175 | Wave 151 component #16/#17; largest remaining pattern duplication after F-3 |
| F-9 | **Dark-mode hardening:** deduplicate the two dark blocks in `tokens.css` (single source — e.g. one `@media` block plus a `[data-theme="dark"]` block generated from the same declarations, shadows included), replace `--p-white` leaks with `--color-on-brand`/`--color-on-accent`, re-point or delete `--color-chart-*`, then decide the toggle question (D-5) explicitly | 175–177 | Must land *before* any theme toggle ships; D-1 becomes a live bug the day one does |
| F-10 | **Retire legacy aliases** (`--bg/--surface/--brand/--border/--radius/--shadow/--font`, ~57 uses) onto semantic names, per the spec's v0.0.4 removal date; update `03-design-systems.md`/DESIGN.md for the motion-token rename and the `--color-border-strong` deviation so the spec stops drifting from the code | 177–179 | Aliases were a one-release bridge (Wave 152); permanent aliases = two vocabularies forever |
| F-11 | **Adopt `StatCard` + `Icon`** on dashboard/index and admin; delete admin's local stat markup/styles | 179–180 | Closes the component library: every component in `src/components/` has ≥1 real consumer, or is deleted |

*— Design Systems Engineer, Wave 165 review. Companion reviews: Review #1 series, Waves 165–166 (same folder).*
