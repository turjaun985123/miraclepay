# 03 — Design Systems Audit (Wave 151)

**Seat:** Design Systems Engineer, MiraclePay 10-agent design engineering team (Waves 151–200)
**Scope inspected:** `frontend/src/layouts/BaseLayout.astro` (171 lines, global styles), `frontend/src/components/DashboardNav.astro` (153 lines, the *only* component), all 15 pages under `frontend/src/pages/**` (9,300 lines total), `frontend/src/lib/api.ts` / `i18n.ts` (no styling, checked for theme hooks — none).
**Method:** full read of `BaseLayout.astro`, `login.astro`, `DashboardNav.astro`; `grep` inventories of every colour / spacing / type / radius / shadow / breakpoint value across all `*.astro`; per-file `<style>` block, class-redefinition, inline-`style=` and markup-pattern counts. All counts below are reproducible with the commands quoted in each section.
**Constraint honoured:** audit only — no source files were edited.

## Verdict in one paragraph

There is a design system, but only a third of the product uses it. `BaseLayout.astro` defines 16 CSS custom properties and 29 global classes (`.card`, `.btn`, `.input`, `.table`, `.badge`, `.alert`, `.stat`, `.empty`, …) and the eight dashboard/admin pages mostly consume them. The three public surfaces were built as three *separate* design systems: the landing page (`index.astro`, 223 style lines) is **violet**, login is **blue**, register is **teal**, and none of the three uses a single `var(--…)` colour. Result: **four different "primary" colours for one product**, 33 distinct hex values, 1,228 lines of CSS spread over 17 files, 8 class names redefined incompatibly in page scope, 51 inline `style=` attributes, and **no dark mode, no modal, no toast, no tabs, no skeleton, no pagination** anywhere. The fix is cheap because the dashboard core is already token-shaped: promote what exists, alias it, and migrate the three outliers.

---

## 1. Inventory — every value currently in use (grep evidence)

### 1.1 The existing token layer (`BaseLayout.astro:27–47`, the only place tokens are defined)

```css
:root {
  --bg:#f6f7fb;  --surface:#ffffff;  --ink:#101828;  --muted:#667085;
  --brand:#0b7a55;  --brand-dark:#095c40;  --brand-soft:#e6f6ef;
  --border:#e4e7ec;
  --danger:#d92d20;  --danger-soft:#fef3f2;
  --warning:#b54708; --warning-soft:#fffaeb;
  --radius:12px;  --radius-sm:8px;
  --shadow:0 1px 2px rgb(16 24 40 / 0.06), 0 1px 3px rgb(16 24 40 / 0.08);
  --font: system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  --font-mono: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
}
```

16 tokens total: 11 colour, 2 radius, 1 shadow, 2 font. **No spacing tokens, no type tokens, no breakpoint tokens, no motion/z-index tokens, no dark variants.** `<meta name="color-scheme" content="light">` (line 22) hard-pins light.

Token *usage* is healthy where it happens (`grep -rho 'var(--…)'`): `--border` ×19, `--muted` ×13, `--surface` ×10, `--radius-sm` ×9, `--brand` ×8, `--brand-dark` ×8, `--brand-soft` ×7, `--bg` ×7, `--ink` ×7, `--shadow` ×5, `--danger` ×4, `--radius` ×3, `--font-mono` ×3, `--danger-soft` ×3, `--warning(-soft)` ×2 each, `--font` ×1.

**Undefined tokens already referenced** (fallbacks silently masking the gap):

| Reference | File | Problem |
|---|---|---|
| `var(--accent,#2563eb)` | `pages/admin/index.astro:601` | `--accent` never defined; admin chart bars are login-blue |
| `var(--brand-green, #0b7a55)` (nested in `var(--brand, …)`) | `pages/pay/[slug].astro:179` | `--brand-green` never defined; legacy name from an earlier rename |
| `var(--danger, #b3261e)` (inline JS) | `pages/dashboard/transactions.astro:329` | Fallback `#b3261e` ≠ `--danger` `#d92d20` — two error reds |
| `var(--radius-sm, 6px)` | `pages/admin/plugins.astro:85` | Fallback 6px ≠ actual 8px — author expected a different scale |

### 1.2 Colour inventory — 33 distinct hex values

`grep -rhoE '#[0-9a-fA-F]{3,8}' --include='*.astro' . | sort | uniq -c | sort -rn`

| Hex | × | Where / role |
|---|---|---|
| `#0b7a55` | 6 | `--brand`; also hard-coded in JS chart fill `dashboard/index.astro:620`, checkout `border-left` fallback, settings colour-picker default `settings.astro:59,62` |
| `#ffffff` / `#fff` | 5 / 4 | `--surface`, `.btn` text, login/register card bg + button text, landing `.btn-primary`/`.step-number` text |
| `#e5e7eb` | 4 | Landing only: header border, feature/step card border, footer border (≈ but ≠ `--border #e4e7ec`) |
| `#2563eb` | 4 | **Login primary** (focus ring ×2, submit bg) + admin `--accent` fallback |
| `#0f766e` | 4 | **Register primary** (brand text, focus ring, border-color, submit bg) |
| `#6d28d9` | 3 | **Landing primary** (`.btn-primary` bg, eyebrow, step-number bg) |
| `#64748b` | 3 | Login/register muted text (≠ `--muted #667085`) |
| `#fef2f2` | 2 | Login/register error bg (≠ `--danger-soft #fef3f2` — off by one digit) |
| `#e2e8f0` | 2 | Login/register card border |
| `#d1d5db` | 2 | Landing button/chip border |
| `#cbd5e1` | 2 | Login/register input border (≠ `--border`) |
| `#b91c1c` | 2 | Login/register error text (≠ `--danger #d92d20`) |
| `#475569` | 2 | Register lede / link text — a third "muted" |
| `#fffaeb`, `#fef3f2`, `#f6f7fb`, `#e6f6ef`, `#e4e7ec`, `#d92d20`, `#b54708`, `#101828`, `#667085`, `#095c40` | 1 each | The `BaseLayout` token definitions themselves |
| `#f9fafb` | 1 | Landing chip bg (≈ but ≠ `--bg #f6f7fb`) |
| `#ecfdf5` + `#065f46` | 1 each | Login "logged in" success note (≈ but ≠ `--brand-soft`/`--brand-dark`) |
| `#1d4ed8` | 1 | Login submit hover (≠ `--brand-dark`) |
| `#115e59` | 1 | Register submit hover |
| `#b42318` | 1 | `BaseLayout` `.btn-danger:hover` — a hover red that exists nowhere as a token |
| `#b3261e` | 1 | Transactions error fallback (see §1.1) |
| `#0e9f6e` | 1 | Only in the Wave-23 comment in `BaseLayout` ("was #0e9f6e ≈ 3.1:1") — dead value, documentation only |

Plus non-hex colour: `rgb(16 24 40 / 0.06)` + `/ 0.08` (`--shadow`), `rgb(15 23 42 / 0.08)` (register card shadow), `rgba(15, 23, 42, 0.08)` (login card shadow — same shadow, legacy syntax), `transparent` ×6, `currentColor` via `color: inherit` ×11.

**Finding C-1 (critical): four primaries.** Green `#0b7a55` (dashboard/checkout/pay/admin), blue `#2563eb` (login), teal `#0f766e` (register), violet `#6d28d9` (landing). A user registers (teal), logs in (blue), lands in a green dashboard they reached from a violet marketing page. Login and register are the *same form* in two different palettes — the clearest possible token failure.
**Finding C-2:** three parallel grey scales coexist — the layout's `#101828/#667085/#e4e7ec/#f6f7fb`, Tailwind-slate `#475569/#64748b/#cbd5e1/#e2e8f0` (auth), Tailwind-gray `#d1d5db/#e5e7eb/#f9fafb` (landing). Every "same" role (border, muted, surface-soft, danger-soft) exists 2–3 times with values 1–3 hex digits apart, i.e. visually indistinguishable but unmaintainable.

### 1.3 Per-file style blocks — where the CSS actually lives

`awk '/<style/,/<\/style>/'` per file; 17 of 18 `.astro` files carry a `<style>` block, **1,228 style lines total**:

| File | Style lines | Unique classes defined | Uses global tokens? |
|---|---|---|---|
| `layouts/BaseLayout.astro` | 141 | 29 | defines them |
| `pages/index.astro` (landing) | 223 | 25 | **No — 0 `var()` colours** |
| `pages/checkout/[id].astro` | 150 | 18 | Yes, mostly |
| `pages/login.astro` | 104 | 9 | **No** |
| `pages/register.astro` | 102 | 8 | **No** |
| `pages/dashboard/api-keys.astro` | 94 | 12 | Yes |
| `pages/dashboard/invoices.astro` | 80 | 9 | Yes |
| `components/DashboardNav.astro` | 68 | 4 | Yes (exemplary) |
| `pages/dashboard/gateways.astro` | 66 | 4 | Yes |
| `pages/pay/[slug].astro` | 61 | 8 | Yes (+ legacy `--brand-green`) |
| `pages/dashboard/payment-links.astro` | 56 | 7 | Yes |
| `pages/admin/plugins.astro` | 25 | 3 | Yes |
| `pages/dashboard/webhooks.astro` | 20 | 1 | n/a (mostly global) |
| `pages/dashboard/transactions.astro` | 14 | 3 | n/a |
| `pages/dashboard/index.astro` | 13 | 1 | n/a — but 22 inline `style=` (see §1.6) |
| `pages/admin/index.astro` | 11 | 1 | n/a |
| `pages/dashboard/settings.astro` | **0** | 0 | n/a — **11 inline `style=`, no style block at all** |

### 1.4 Typography inventory

`grep -rhoE 'font-size:[^;]+;' | sort | uniq -c` — 13 distinct sizes, no scale token:

| Size | × | Roles |
|---|---|---|
| `0.875rem` (14px) | 9 | `.small`, labels, hints, nav links, register hint |
| `0.8125rem` (13px) | 7 | `.stat-label`, table `th`, `.field-hint`, badge, plugin output |
| `0.9375rem` (15px) | 6 | body-adjacent: `.btn`, `.input`, `.table`, check-rows |
| `1.5rem` (24px) | 5 | auth `h1`, mobile `h1` |
| `1.375rem` (22px) | 4 | `h2`, checkout `h1`, landing logo |
| `1rem`, `1.75rem` | 2 each | new-key value / `h1`, pay amount |
| `1.125rem` | 2 | `h3`, gateway name |
| `clamp(2.25rem, 6vw, 3.75rem)` | 1 | Landing hero only — the only fluid size |
| `16px` | 1 | `body` in BaseLayout (the only px font-size) |
| `1.625rem`, `1.25rem`, `0.9em` | 1 each | `.stat-value`, hero sub, `code` |

Weights: `600` ×19, `700` ×11, `650` ×2 (headings + gateway name — a variable-font weight on a system-ui stack, silently rounds), `800` ×1 (landing logo). Line-heights: `1.55` body, `1.6` ×3, `1.5` ×2, `1.25` ×2, `1.15` ×2, `1.2`, `1.05`, `1` — 8 values. Letter-spacing: `0.04em` ×3, `0.02em` ×3, `0.08em`, `0.05em`, `0.03em`, `-0.02em`, `-0.03em` — 7 values. Font stacks: exactly 2 (`--font`, `--font-mono` ×3 uses); **no Bangla font consideration** despite `lib/i18n.ts` shipping a full `bn` locale — Bengali text falls back to whatever the OS picks, unreviewed.

### 1.5 Spacing inventory

No spacing tokens exist; values are raw `rem`. Distinct values actually used across `padding`/`margin`/`gap` (grep counts):

- **Padding** (29 distinct declarations): `0` ×13, `0.75rem 1rem` ×5, `0.625rem 0.75rem` ×5, `1.25rem` ×4, `2rem` ×2, `2rem 1rem` ×2, `0.875rem 1rem` ×2, `0.5rem` ×2, `0.5rem 1rem` ×2, `0.375rem 0.75rem` ×2, then 19 one-offs incl. `4.5rem 0 3rem` (hero), `2.5rem 1rem` (`.empty`), `0.5625rem 1rem` / `0.5625rem 0.75rem` (button/input — 9px, off any 4px grid), `0.125rem 0.625rem` (badge).
- **Margin**: `0` ×27, `0 0 1rem` ×8, `margin-bottom:1rem` ×7, `0 auto` ×4, `1.5rem 0 0` ×4, `margin-top:1rem` ×4, plus ~25 one-offs down to `0.125rem` and `-1px` (sr-only).
- **Gap**: `1rem` ×8, `0.5rem` ×9 (incl. one `gap:0.5rem` unspaced), `0.75rem` ×3, `0.625rem` ×3, `0.375rem` ×3, `0.25rem` ×3, `1.5rem`, `1.25rem`, `0.875rem`, `0.3125rem` (badge icon gap — 5px, off-grid).
- **Widths**: container `1120px` (BaseLayout) vs `72rem` = 1152px (landing, ×3) vs `560px` (checkout/pay) vs `42rem`/`40rem`/`26rem`/`24rem`/`22rem`/`16rem`/`14rem`/`12rem`/`10rem`/`360px`/`320px`/`220px` — **13 container/measure widths, mixed px/rem, no token**. Grid tracks: `minmax(220px,1fr)`, `230px`, `240px`, `200px`, `180px`, `15rem`, and invoice item-row's magic `1fr 90px 140px auto`.

The de-facto rhythm is a 4px (0.25rem) grid with five recurring offenders: `0.5625` (9px), `0.3125` (5px), `0.875` (14px), `0.625` (10px), `2.5/4.5rem` section jumps.

### 1.6 Inline styles — 51 `style="…"` attributes

`grep -rho 'style="[^"]*"'` per file: `dashboard/index.astro` **22**, `settings.astro` **11**, `webhooks.astro` 9, `admin/index.astro` 6, `pay/[slug]` 1, `checkout/[id]` 1, `DashboardNav` 1 (the language toggle's `background:none;border:none;cursor:pointer;font:inherit` — a button reset re-invented inline), all other pages 0.
Most-repeated inline declarations: `margin: 0` ×17, `justify-content: space-between` ×5, `padding: 0` ×4, `margin-bottom: 0` ×4, `list-style: none` ×4, `width: auto` ×2, `min-width: 220px` ×2, `float:right` ×2 (legacy layout in 2026 CSS), plus one-off `max-width: 10/12/14/16rem`, `grid-template-columns:1fr 1fr`, `flex: 1 1 240px`. These are not dynamic values — they are missing utility/component classes (`.page-head`, `.list-reset`, `.field--flush`, `.input--auto`). The genuinely dynamic ones (admin bar-chart `width:${…}%`, `height:0.8rem`) are legitimate inline styles and should stay.

### 1.7 Radius, shadow, breakpoints, motion, z-index

| Property | Values in use | Verdict |
|---|---|---|
| `border-radius` | `var(--radius-sm)` ×9, `0.5rem` ×8 (= 8px, same value, un-tokenised — all in auth/landing), `999px` ×7, `0.75rem` ×4 (= 12px = `--radius`, un-tokenised), `var(--radius)` ×3, `var(--radius-sm, 6px)` ×1 | Two names for every value; token coverage ~40% |
| `box-shadow` | `var(--shadow)` ×4, `0 10px 30px rgb(15 23 42 / .08)` (register), `0 10px 25px rgba(15, 23, 42, .08)` (login) | One "elevated card" shadow, written twice, two syntaxes, two blur radii |
| `@media` breakpoints | `max-width: 640px` ×2 (BaseLayout, +1 more), `max-width: 40rem` ×1 (landing = same 640px, different unit), `prefers-reduced-motion` ×1 (BaseLayout — good) | **One real breakpoint, spelled two ways; no tablet/desktop tiers** |
| `transition` | `background 0.15s ease` (btn), `top 0.15s ease` (skip-link), multi-property in nav/gateway-card | One duration (150ms) de-facto; not tokenised |
| `z-index` | `100` ×1 (skip-link) | No scale; first modal/toast will invent one |
| `color-scheme` | `light` only | Dark mode structurally blocked |

### 1.8 Class collisions and repeated markup patterns

**Redefined classes** (same name, different rules, page-scoped — Astro scoping prevents a crash today, guarantees drift tomorrow): `.field` defined **4×** (BaseLayout = block + `margin-bottom:1rem`; invoices; login = flex-column; register = margin only — and login/register inputs don't even use the global `.input` class, styling bare `input` instead), `.btn` 2× (BaseLayout vs landing, incompatible: `inline-flex`+green vs `inline-block`+violet), `.badge` 2×, `.input` 2×, `.stat-value` 2×, `.form-grid` 2× (invoices/payment-links, copy-pasted identical), `.list-header` 2× (same), `.check-row` 2× (**different**: gateways = grid, payment-links = flex).

**Copy-pasted logic/markup patterns** (grep evidence):

1. **Status→badge mapping, implemented 5 times:** `transactions.astro:222`, `invoices.astro:552` (identical `badgeClass()` functions), `dashboard/index.astro:366–376`, `payment-links.astro:476–482`, `api-keys.astro:451–455` (a status→`{label, cls}` map). One `statusBadge(status)` helper replaces all five.
2. **Loading paragraph, 13 pages:** `<p id="…-loading" class="muted">Loading …</p>` + `loadingEl.hidden` toggling in api-keys, gateways, dashboard, invoices, payment-links, transactions, webhooks, settings, admin, plugins, checkout, pay — three different id conventions, no spinner, no skeleton, no `aria-busy`.
3. **Empty state:** `.empty` used 19×, but text/markup hand-written each time ("No transactions yet…", "No invoices yet. Create your first invoice above.") with no shared icon/CTA structure.
4. **Page header:** every dashboard page hand-builds `<div class="row" style="justify-content: space-between"><h1>…</h1>…merchant <select>` — copy-pasted in dashboard/index, settings, webhooks (×3 as section headers) et al.
5. **Alerts:** `.alert` ×53 uses (`alert-error` ×33, `alert-success` ×14) — the most-used feedback pattern, yet always a static inline `<div hidden>`, never dismissible, never announced consistently (`role="alert"` present on some, absent on others).
6. **Forms:** `.field` ×93, `.label` ×71, `.input` ×61 uses — the strongest existing pattern; invoices/payment-links additionally duplicate `.form-grid` + fieldset styling (`items-fieldset`, `collect-fieldset`, `scope-fieldset` — three names, one job).
7. **Visually-hidden, twice:** `.visually-hidden` (api-keys) and `.sr-only` (webhooks) — same utility, two names, slightly different recipes; BaseLayout has neither.
8. **Copy-to-clipboard / secret reveal:** hand-rolled separately in api-keys (new-key box) and payment-links (pay URL); settings re-implements constrained inputs via inline `max-width` ×4.
9. **Confirmation:** exactly one destructive-action guard in the whole app — `window.confirm(...)` at `api-keys.astro:510`. Revoke/delete elsewhere is unguarded or ad hoc.
10. **Absent entirely** (grep = 0): `<dialog>`/modal, toast, `role="tab"`/tabs, skeleton, pagination, tooltip, breadcrumb, avatar, progress (except the admin inline bar chart), spinner.

### 1.9 What is already good (keep, don't redesign)

`DashboardNav.astro` is fully token-driven and is the model component. BaseLayout already ships a skip link, `:focus-visible` ring, `prefers-reduced-motion` block, WCAG-AA brand green (Wave 23, `#0e9f6e`→`#0b7a55` ≈5:1), sentence-case uppercase-labelled table headers, and a coherent badge/alert vocabulary. Colours `#101828/#667085/#e4e7ec` are a sound neutral ramp. The token proposal below therefore *extends* this file rather than replacing it.

---

## 2. Proposed token system

### 2.1 Architecture — three layers, one file, no build tooling

Astro 7 static output, no CSS framework today and none proposed. One new file, `frontend/src/styles/tokens.css`, imported once from `BaseLayout.astro`; page `<style>` blocks may consume tokens but may not define colour/spacing literals (enforced by the CI grep gate in §4).

1. **Primitive layer** (`--p-*`): raw ramps. Only layer allowed to contain hex.
2. **Semantic layer** (`--color-*`, `--space-*`, …): what components reference. Dark mode re-points *only this layer*.
3. **Component layer** (`--btn-*`, `--input-*`, …): optional, added per component in §3 when a component needs a knob.

**Back-compat aliases are mandatory:** `--bg`, `--surface`, `--ink`, `--muted`, `--brand`, `--brand-dark`, `--brand-soft`, `--border`, `--danger`, `--danger-soft`, `--warning`, `--warning-soft`, `--radius`, `--radius-sm`, `--shadow`, `--font`, `--font-mono` all keep working as aliases of the new names for one release (§4, Phase 1), so no page breaks on day one.

### 2.2 Colour palette (concrete values)

Decision on Finding C-1: **brand green `#0b7a55` is the single product primary** — it is the palette with the audited WCAG fix, it is what merchants see 95% of the time, and it is merchant-configurable at checkout (`settings.astro` brand-colour picker defaults to it). Blue `#2563eb` and teal `#0f766e` are **retired** (login/register adopt green). Violet `#6d28d9` is **demoted, not deleted**: it becomes `--color-accent` for marketing-only emphasis on the landing page (eyebrow, step numbers) and may never style an interactive control — landing CTAs become green like every other CTA.

Primitives (ramps anchored on values already in the codebase):

```css
--p-green-700:#095c40; --p-green-600:#0b7a55; --p-green-500:#0e9f6e; --p-green-50:#e6f6ef;
--p-violet-700:#6d28d9;
--p-red-700:#b42318;   --p-red-600:#d92d20;   --p-red-50:#fef3f2;
--p-amber-700:#b54708; --p-amber-600:#dc6803; --p-amber-50:#fffaeb;
--p-blue-600:#2563eb;  --p-blue-50:#eff6ff;            /* info only — never primary */
--p-gray-900:#101828; --p-gray-700:#344054; --p-gray-500:#667085; --p-gray-400:#98a2b3;
--p-gray-300:#d0d5dd; --p-gray-200:#e4e7ec; --p-gray-100:#f2f4f7; --p-gray-50:#f6f7fb;
--p-white:#ffffff;
```

Semantic layer — light (default) and dark:

| Token | Light | Dark | Notes / replaces |
|---|---|---|---|
| `--color-bg` | `#f6f7fb` (gray-50) | `#0b1220` | `--bg` |
| `--color-surface` | `#ffffff` | `#101828` (gray-900) | `--surface` |
| `--color-surface-raised` | `#ffffff` | `#1d2939` (gray-800) | modal, toast, dropdown |
| `--color-surface-sunken` | `#f2f4f7` (gray-100) | `#0b1220` | code/plugin output, table hover |
| `--color-ink` | `#101828` | `#f2f4f7` | `--ink` |
| `--color-ink-2` | `#344054` | `#d0d5dd` | **new** — absorbs register's `#475569` body-secondary |
| `--color-muted` | `#667085` | `#98a2b3` | `--muted`; absorbs `#64748b` |
| `--color-border` | `#e4e7ec` | `#344054` | `--border`; absorbs `#e5e7eb`, `#e2e8f0`, `#d1d5db` |
| `--color-border-strong` | `#d0d5dd` | `#475467` | **new** — absorbs input border `#cbd5e1` |
| `--color-brand` | `#0b7a55` | `#34c98e` | dark value lightened to keep ≥4.5:1 on gray-900 |
| `--color-brand-hover` | `#095c40` | `#5ed6a5` | `--brand-dark` (rename: it is a hover state, not just "dark") |
| `--color-brand-soft` | `#e6f6ef` | `#0c2b21` | `--brand-soft`; absorbs login's `#ecfdf5` |
| `--color-on-brand` | `#ffffff` | `#06231a` | button text on brand |
| `--color-accent` | `#6d28d9` | `#a78bfa` | marketing only (landing) |
| `--color-danger` | `#d92d20` | `#f97066` | `--danger`; absorbs `#b91c1c`, `#b3261e` |
| `--color-danger-hover` | `#b42318` | `#f04438` | promotes the orphan `.btn-danger:hover` value to a token |
| `--color-danger-soft` | `#fef3f2` | `#3a1512` | absorbs `#fef2f2` |
| `--color-warning` | `#b54708` | `#fdb022` | `--warning` |
| `--color-warning-soft` | `#fffaeb` | `#342605` | `--warning-soft` |
| `--color-info` | `#2563eb` | `#84adff` | **new semantic** — gives admin's `--accent` fallback a real home (chart/info), without making blue a CTA colour |
| `--color-info-soft` | `#eff6ff` | `#10264d` | |
| `--color-focus` | `#0b7a55` | `#34c98e` | focus ring; retires login's blue ring and register's teal ring |
| `--color-overlay` | `rgb(16 24 40 / .48)` | `rgb(0 0 0 / .64)` | **new** — modal scrim, does not exist today |
| `--color-chart-1..5` | `#0b7a55, #2563eb, #6d28d9, #dc6803, #667085` | lightened equivalents | **new** — admin dashboard chart + JS `fill:'#0b7a55'` read these via `getComputedStyle` |

Dark-mode activation (both, in this order): `@media (prefers-color-scheme: dark)` under `:root:not([data-theme="light"])`, plus an explicit `[data-theme="dark"]` override for a future settings toggle; `<meta name="color-scheme" content="light dark">`. Merchant checkout branding (`--brand` set inline from the settings colour picker on `checkout/[id]` and `pay/[slug]`) overrides `--color-brand*` at page scope in both modes — the token layer must compute hover/soft from it, so define those two as `color-mix(in srgb, var(--color-brand) 82%, black)` / `… 10%, white` with the static values above as the default-brand fallbacks.

### 2.3 Type scale

Base 16px, ratio ≈1.2 (minor third), weights restricted to **400 / 600 / 700** (drop `650` and `800`; system-ui has no reliable 650).

| Token | Value | Line-height token | Replaces |
|---|---|---|---|
| `--text-xs` | `0.75rem` (12px) | `--lh-tight 1.25` | new floor for chart/legend text |
| `--text-sm` | `0.8125rem` (13px) | `1.4` | stat-label, `th`, field-hint, badge (13px ×7 today) |
| `--text-base-sm` | `0.875rem` (14px) | `1.5` | `.small`, labels, nav (×9) |
| `--text-body` | `0.9375rem` (15px) | `1.55` | btn/input/table text (×6) — kept deliberately: it is the app's real body size |
| `--text-base` | `1rem` (16px) | `1.55` | body, key values |
| `--text-lg` | `1.125rem` (18px) | `1.4` | `h3`, gateway name |
| `--text-xl` | `1.375rem` (22px) | `1.25` | `h2`, checkout `h1` |
| `--text-2xl` | `1.5rem` (24px) | `1.2` | auth `h1` |
| `--text-3xl` | `1.75rem` (28px) | `1.15` | `h1`, `.stat-value` (absorbs 1.625rem), pay amount |
| `--text-display` | `clamp(2.25rem, 6vw, 3.75rem)` | `1.05` | landing hero (already exactly this) |

Also: `--font-sans` (= today's `--font`) **plus a Bangla fallback appended** — `…, "Hind Siliguri", "Noto Sans Bengali", sans-serif` — so the shipped `bn` locale renders in a designed face where available instead of an arbitrary OS fallback; `--font-mono` unchanged; `--tracking-caps: 0.04em` (stat labels, eyebrows — absorbs 0.02/0.03/0.05/0.08em sprawl: use `0.04em` for labels, `--tracking-tight: -0.02em` for display/logo only).

### 2.4 Spacing scale — 4px base

`--space-0:0; --space-px:1px; --space-0_5:0.125rem; --space-1:0.25rem; --space-1_5:0.375rem; --space-2:0.5rem; --space-2_5:0.625rem; --space-3:0.75rem; --space-3_5:0.875rem; --space-4:1rem; --space-5:1.25rem; --space-6:1.5rem; --space-8:2rem; --space-10:2.5rem; --space-12:3rem; --space-18:4.5rem`.

Every value in §1.5 maps onto this scale with two exceptions, which are the point of the exercise: `0.5625rem` (9px, button/input vertical padding) → `--space-2_5` block + line-height adjustment, or explicitly `--btn-pad-y:0.5625rem` as a *component* token if pixel-parity is required for the v0.0.3 visual gate; and `0.3125rem` (5px, badge gap) → `--space-1`. Component spacing aliases: `--pad-card: var(--space-5)`, `--pad-card-lg: var(--space-6)`, `--gap-grid: var(--space-4)`, `--section-y: var(--space-10)`.
Layout tokens: `--container: 70rem` (1120px, the dashboard value — landing's 72rem/1152px converges to it), `--container-narrow: 35rem` (560px, checkout/pay), `--container-auth: 26rem` (register's 26rem wins over login's 24rem — one auth width), `--measure: 42rem` (prose).

### 2.5 Radius, shadow, breakpoints, motion, z-index

```css
--radius-none:0; --radius-xs:4px; --radius-sm:6px; --radius-md:8px;  /* = today's --radius-sm */
--radius-lg:12px; /* = today's --radius */  --radius-xl:16px; --radius-full:999px;
--shadow-xs: 0 1px 2px rgb(16 24 40 / .06);
--shadow-sm: 0 1px 2px rgb(16 24 40 / .06), 0 1px 3px rgb(16 24 40 / .08); /* = today's --shadow */
--shadow-md: 0 4px 8px -2px rgb(16 24 40 / .10), 0 2px 4px -2px rgb(16 24 40 / .06);
--shadow-lg: 0 10px 25px rgb(15 23 42 / .08);  /* login/register card — pick 25px, retire 30px twin */
--shadow-xl: 0 20px 40px -12px rgb(16 24 40 / .25); /* modal/toast */
--duration-fast:150ms; --duration-med:250ms; --ease: cubic-bezier(.2,0,0,1);
--z-base:0; --z-sticky:50; --z-nav:100 /* skip-link today */; --z-dropdown:200; --z-overlay:300; --z-modal:310; --z-toast:400;
```
Dark shadows: same geometry, `rgb(0 0 0 / .4)` colour.
**Breakpoints** (CSS variables cannot drive `@media`, so these are named constants documented in `tokens.css` as comments and used literally — one spelling, `px`, everywhere): `--bp-sm: 480px`, `--bp-md: 640px` (today's only breakpoint, both spellings converge here), `--bp-lg: 900px` (tables/nav collapse), `--bp-xl: 1120px` (= container). Landing's `40rem` becomes `640px`.

---

## 3. Proposed component inventory

Conventions: **Status** = Exists-global (in BaseLayout) / Page-local (exists but duplicated or divergent) / Missing. Components are `.astro` files in `frontend/src/components/` with typed `Props`, plus two JS helpers in `frontend/src/lib/` where the pattern is rendered from client script (most dashboard markup is JS-rendered, so a purely Astro component library would miss half the UI — `statusBadge()` and `toast()` must be importable from scripts). Priorities: **P0** = v0.0.3 design release blocker, **P1** = same program, **P2** = fast follow.

| # | Component | Status today | Pages that need it | Variants / props | Pri |
|---|---|---|---|---|---|
| 1 | **Button** (`Button.astro` + `.btn` classes) | Exists-global, but landing redefines `.btn`/`.btn-primary`/`.btn-secondary`; `.btn-danger` defined yet **0 uses** | All 15 — 39 `.btn` + 14 `.btn-ghost` + 3 `.btn-sm` uses today; landing, login, register first | `variant: primary\|ghost\|danger\|link`, `size: sm\|md\|lg`, `loading`, `block` (pay-submit is a full-width button re-implemented locally) | P0 |
| 2 | **Field + Input** (`Field.astro`) | Exists-global (`.field/.label/.input/.field-hint`, 93/71/61 uses) but login/register bypass it with bare `input` styles and a rival `.field` | login, register, dashboard ×7, admin, checkout, pay | wraps label+hint+error; `error` text + `aria-invalid`/`aria-describedby` built in (today hand-wired per form); sizes for inline filters | P0 |
| 3 | **Select** | Page-local: `.input` on `<select>` + inline `style="width:auto;min-width:220px"` ×2 (merchant picker, copy-pasted in dashboard/index + settings), `admin/index` re-styles `select.input` | dashboard/index, settings, transactions (filters), admin, webhooks | `inline` variant (= the merchant picker, becomes `MerchantSelect` usage), chevron via background SVG token | P0 |
| 4 | **Textarea** | Exists-global in CSS (`textarea.input`), near-zero distinct styling | settings, webhooks, invoices (notes) | autosize off; mono variant for payloads/secrets | P1 |
| 5 | **CheckboxRow / CheckboxGroup** (`Fieldset.astro`) | Page-local ×3 divergent: `.check-row` grid (gateways) vs flex (payment-links), `.scope-grid/.scope-option` (api-keys), `.collect-fieldset` / `.items-fieldset` / `.scope-fieldset` | gateways, payment-links, api-keys, invoices, settings, dashboard (2FA/KYC) | `fieldset+legend` semantics preserved; `accent-color: var(--color-brand)` standardised | P0 |
| 6 | **Card** (`Card.astro`) | Exists-global, 59 uses — the healthiest component; checkout/pay/auth/landing each hand-roll a variant | All 15 | `pad: md\|lg`, `elevated` (= `--shadow-lg`, auth cards), `interactive` (= gateway-card hover/selected), `as: section\|article\|button` | P0 |
| 7 | **StatCard** | Exists-global (`.stat`, 34 uses) + admin redefines `.stat-value` | dashboard/index, admin/index, admin/plugins | `label, value, hint, trend?`; overflow-wrap fix from admin promoted | P0 |
| 8 | **PageHeader** | Missing as a component; markup copy-pasted with inline styles on ≥5 pages (§1.8.4) | dashboard ×7, admin ×2 | `title, lede?, actions slot` — kills ~15 inline `style=` in one move | P0 |
| 9 | **Table** (`DataTable.astro` shell) | Exists-global (`.table/.table-wrap`, 26/13 uses) but every page hand-renders rows + loading + empty in JS | transactions, invoices, payment-links, api-keys, webhooks ×2 tables, admin ×5 tables | shell owns `<table-wrap>`, sticky header at `--bp-lg`, numeric/`mono` cell alignment, row-action slot; rendering stays per-page | P0 |
| 10 | **StatusBadge** (`lib/status.ts` → `statusBadge(status)`) | Page-local logic ×5 (§1.8.1); `.badge-*` classes exist-global but only 5 static uses — almost all badges are JS-built | transactions, invoices, payment-links, api-keys, gateways, dashboard/index, checkout, webhooks | one status→`{tone, label}` map: `success\|pending\|failed\|neutral\|info`; tones = the 4 existing classes + neutral; kills 5 duplicate functions | P0 |
| 11 | **Badge** (non-status) | Exists-global (`.badge`) | gateways (country/sandbox), api-keys (scopes), dashboard (live indicator), checkout (status) | `tone`, `dot?` (live indicator), removable (filter chips — new) | P1 |
| 12 | **Alert** | Exists-global, 53 uses — most-used feedback UI, but static, non-dismissible, inconsistent `role` | All dashboard pages, checkout, pay, auth (as `.login-error`/`.error` locally today) | `tone: error\|success\|warning\|info`, `dismissible`, live-region behaviour fixed per tone (`alert` vs `status`) | P0 |
| 13 | **Toast** (`lib/toast.ts`) | **Missing (0)** — every async success/failure today either navigates, edits a page-level alert, or says nothing | api-keys (key created/copied/revoked), payment-links (copied), webhooks (saved/test), settings (saved), gateways (saved), invoices | stacked, auto-dismiss 5s, `role=status/alert` by tone, `--z-toast`, pause-on-hover; replaces ≥8 page-level success alerts over time | P0 |
| 14 | **Modal / Dialog** (`Dialog.astro` on native `<dialog>`) | **Missing (0)**; only guard is `window.confirm` at `api-keys.astro:510` | api-keys (revoke confirm + new-key reveal), webhooks (delete + delivery payload), invoices (void), admin (suspend/payout approve), settings (2FA setup — today an inline card), dashboard (create-payment result) | `ConfirmDialog` preset (danger confirm, focus trap + return, Esc) and free-form; `--color-overlay`, `--z-modal`, `--shadow-xl` | P0 |
| 15 | **Tabs** | **Missing (0)** — webhooks fakes it with stacked sections, settings with stacked cards, admin with one 827-line scroll | webhooks (Endpoints / Events / Deliveries), settings (Profile / Branding / Notifications / Payouts / Security), admin/index (Users / Merchants / Analytics / Audit / Disputes), plugins (Config / Logs) | URL-hash synced, `role=tablist` keyboard model; biggest IA win in the program | P1 |
| 16 | **Skeleton** (`Skeleton.astro`) | **Missing (0)** — replaced by 13 bespoke "Loading…" paragraphs (§1.8.2) | All 13 data pages; shapes: `table` (5 rows), `card`, `stat`, `text` | `aria-busy` on region, respects `prefers-reduced-motion` (no shimmer), replaces loading `<p>` page by page | P0 |
| 17 | **EmptyState** | Page-local: `.empty` class exists-global (19 uses) but structure/text/CTA hand-written each time | transactions, invoices, payment-links, api-keys, webhooks, admin (analytics/audit), dashboard (search results ×3) | `title, body?, action?` (Button slot), optional icon; search variant ("No matching …" + clear-filters action) | P0 |
| 18 | **Pagination** | **Missing (0)** — lists silently truncate / load-all | transactions, webhooks events+deliveries, admin audit, invoices | `page, total, onNavigate`; simple prev/next + count first (numbered = P2) | P1 |
| 19 | **SearchBar / FilterBar** | Page-local: dashboard global search (own markup + 3 result lists), transactions `.tx-filter-*` classes | dashboard/index, transactions, admin, webhooks | input + submit + clear; FilterBar = row of Fields + actions (absorbs `.tx-filter-row/-field/-actions`) | P1 |
| 20 | **CopyField / SecretReveal** | Page-local ×2 (api-keys new-key box, payment-links pay-URL) + settings TOTP secret as plain `.mono` text | api-keys, payment-links, webhooks (signing secret), settings/dashboard (TOTP secret + URI) | masked/unmasked toggle, copy Button with Toast feedback, `user-select:all` fallback, warning Alert slot (api-keys' `.new-key-box` becomes this + `tone=warning`) | P0 |
| 21 | **ProgressBar / Steps** | Page-local: admin bar chart (inline styles + undefined `--accent`), dashboard onboarding checklist (badges as step marks) | admin/index (analytics), dashboard/index (onboarding), checkout (payment progress — new) | `value/max`, tone from `--color-chart-*`; Steps: `done\|current\|todo` | P1 |
| 22 | **DefinitionList / Totals** | Page-local ×3 near-identical: invoices `.totals`, checkout `.checkout-summary/.checkout-row`, pay `.pay-details`, dashboard `.result-grid` | invoices, checkout, pay, dashboard (payment result), admin | `rows: {dt, dd, strong?}`, grand-total row, mono numerals | P1 |
| 23 | **AuthCard / AuthLayout** | Page-local ×2 divergent (login `.login-card` blue, register `.auth-card` teal — different widths 24 vs 26rem, heights 70 vs 100vh) | login, register (+ future reset/verify pages) | one centred Card + brand line + footer link; deletes ~200 lines of duplicate CSS | P0 |
| 24 | **SiteHeader / SiteFooter** (marketing) | Page-local, landing only (`.site-header/.nav/.site-footer`, violet) | index (+ future docs/pricing pages) | reuses Button + container token; accent restricted per §2.2 | P1 |
| 25 | **DashboardNav** | **Exists as the sole component — keep as the reference implementation** | dashboard ×7, admin ×2 | extract the language toggle's inline reset into Button `variant=link`; add overflow fade at `--bp-md` | P1 |
| 26 | **VisuallyHidden** utility | Page-local ×2 names (`.visually-hidden`, `.sr-only`) | api-keys, webhooks, + all future components | single `.visually-hidden` in the global sheet; delete both locals | P0 |
| 27 | **Spinner** | Missing (loading text only) | Button `loading`, Skeleton fallback, checkout/pay polling states | 16/20px, `currentColor`, reduced-motion = static | P1 |
| 28 | **Tooltip, Breadcrumb, Avatar, Menu/Dropdown** | Missing (0) | Menu: table row actions (transactions, invoices, admin) once Pagination/Tabs land; others: no current need | — explicitly **deferred**: do not build speculatively in v0.0.3 | P2 |

**Net component count for v0.0.3:** 20 to build/unify (11 P0), 2 helpers (`statusBadge`, `toast`), 1 existing to keep (`DashboardNav`), 4 explicitly deferred. Estimated deletion: landing+auth duplicate CSS ≈ 380 lines, `.form-grid/.list-header/.check-row` duplicates ≈ 60 lines, inline-style utilities ≈ 35 attributes.

---

## 4. Migration strategy — build green at every step

**Invariant:** `npm run build` (Astro 7.4.13, static output) and `npm run check` in `frontend/` pass after *every* phase and every page PR; no phase changes markup semantics and visuals in the same commit as token plumbing. Each phase is independently revertible.

**Phase 0 — Baseline (no code change to pages).** Snapshot the current `frontend/dist/` (15 pages) as the visual baseline; add a screenshot script (dashboard, login, register, landing, checkout, pay at 1280px + 390px) stored outside the repo. Record the §1 grep counts as the scoreboard. *Gate:* baseline committed to the design-audit folder; build green (it is today — `dist/` is current).

**Phase 1 — Tokens land, nothing moves.** Create `src/styles/tokens.css` with §2 primitives + semantics for **light only**, where every semantic token's value is *exactly* today's value (`--color-brand:#0b7a55`, `--color-border:#e4e7ec`, …), plus the §2.1 alias block mapping all 16 legacy names onto them. `BaseLayout.astro` imports it and its inline `:root` block is deleted. Also add `--color-info`/`--accent` definition (fixes admin chart's silent fallback without changing its colour — `#2563eb` becomes the *defined* value) and the global `.visually-hidden`. *Gate:* build + check green; `dist/` CSS diff shows only variable renaming; screenshot diff = 0 pages changed. This is the only phase that touches every page's output, and it must be a no-op visually.

**Phase 2 — Kill the three rogue palettes (one PR per page, in this order).** `register.astro` → `login.astro` → `index.astro`: replace each page-local colour/spacing/radius literal with tokens, adopt global `.btn/.input/.field/.alert`, delete the local `.field`/`.btn` redefinitions. Register and login converge onto the shared AuthCard markup in the same PRs (they are the same form; doing tokens without markup would preserve the duplication). Landing keeps violet *only* via `--color-accent` on non-interactive elements. *Gate per PR:* build green; screenshot diff limited to the intended colour convergence (teal/blue→green) on that one page; grep count of hex literals outside `tokens.css` drops by that page's §1.2 rows (register −11, login −10, landing −8).

**Phase 3 — Utilities for inline styles.** Add global utility classes for the §1.6 repeat offenders (`.page-head` flex-between row, `.list-reset`, `.field--flush`, `.input--auto`, `.mt-4`-style spacing utilities generated from the `--space-*` scale) and sweep `dashboard/index` (22), `settings` (11), `webhooks` (9), `admin/index` (6). Dynamic inline styles (chart widths) stay. *Gate:* build green; `grep -c 'style="'` per file ≤1 (DashboardNav's toggle is fixed by Button `variant=link` in Phase 4); screenshots unchanged.

**Phase 4 — Components, P0 first, one component per PR.** Order by blast radius ÷ risk: `statusBadge()` helper (pure JS, 5 call sites, unit-checkable by grep) → `VisuallyHidden` cleanup → `PageHeader` → `EmptyState` → `Skeleton` (swap loading `<p>`s page by page, keeping the same element ids so page scripts' `loadingEl.hidden` toggles keep working — Skeleton renders *into* that id) → `CopyField` → `Alert` semantics → `Button/Input/Select/CheckboxGroup` Astro wrappers (markup-only wrappers around the existing global classes, so JS-rendered rows keep using the classes directly) → `Card/StatCard/DataTable` shells → `Dialog` (replace `window.confirm` at `api-keys.astro:510` first, then the new-key reveal) → `Toast` (wire to copy/save actions last, once Dialog exists). *Gate per PR:* build + check green; the touched page's screenshot diff shows only the intended component delta; no page script's DOM ids/classes contract changes without the script updated in the same PR.

**Phase 5 — Dark mode (opt-in, last, separately shippable).** Flip on the §2.2 dark semantic layer (`prefers-color-scheme` + `color-scheme: light dark`), audit merchant-branded checkout/pay pages against arbitrary merchant `--color-brand` values in both modes, add the settings toggle (`data-theme`) only if UX Research's Wave 151 findings ask for it. *Gate:* build green; full screenshot pass in both schemes; contrast spot-check of the five soft/ink pairs. Dark mode ships dark-launch (media query only) if any page fails — the token layer makes that a one-line change.

**Standing guardrails (add in Phase 1, enforce from Phase 2):**
- CI grep gate: `grep -rnE '#[0-9a-fA-F]{3,8}' frontend/src --include='*.astro' | grep -v styles/tokens.css` must return only the allow-listed exceptions (settings colour-picker default, JS chart fallback) — list them in the gate script with file:line, so any *new* hard-coded colour fails CI.
- Same gate for `style="` count per file (ratchet: may only decrease) and for new `<style>` blocks defining `.btn/.field/.input/.badge/.card/.table/.alert` outside `BaseLayout`/components.
- No Tailwind/framework adoption mid-program; no renaming of the 16 legacy tokens until v0.0.4 (aliases deprecated in 0.0.3 docs, removed in 0.0.4).
- Bangla (`bn`) pass is part of Phase 4 acceptance for every text-bearing component: `--font-sans` Bengali fallbacks (§2.3) + a screenshot of login/dashboard in `bn` per component PR.

**Risks named:** (a) Astro scoped styles — page `<style>` blocks are scoped, global classes live in the layout, so extracting `tokens.css` must be verified in built `dist/` HTML on *every* page, not assumed (Phase 1 gate does this). (b) JS-rendered markup (most dashboard rows/badges) bypasses `.astro` components — hence helpers-first ordering in Phase 4. (c) Merchant brand colour at checkout is a runtime override of the token layer; `color-mix` derivation (§2.2) needs a fallback check in the oldest supported browser before Phase 5. (d) The v0.0.3 visual-regression gate (Design QA seat) should sign off Phases 2 and 5 diffs specifically — they are the only intentionally visual phases.

*— Design Systems Engineer, Wave 151 audit. Companion audits: 01 UX Research, 02 Visual, 04 Dashboard UI, 05 Checkout UX (same folder).*
