# 10 — QA / Visual Regression Audit (Wave 151)

Role: QA / Visual Regression Engineer, 10-agent design engineering team (Waves 151–200)
Scope inspected (audit only, no source edits): `frontend/package.json`,
`frontend/astro.config.mjs`, `frontend/src/layouts/BaseLayout.astro`,
`frontend/src/components/DashboardNav.astro`, all 15 pages under
`frontend/src/pages/`, `frontend/src/lib/{api,i18n}.ts`, `.github/workflows/ci.yml`.
Verified firsthand 2026-11-26: `npm run build`, `npm run check`, grep counts below.
Standing rule applied throughout: **no rendered screenshots may be pushed to
GitHub.** Every visual check proposed here is local-only, CI-ephemeral
(run artifacts with short retention, never committed), or pure code/DOM/CSS
assertion. No screenshot, snapshot image, or golden PNG is to land in the repo.

---

## 1. Current verification state

### 1.1 Build setup — what exists

| Fact | Evidence |
|---|---|
| Stack | Astro `^7.4.13`, `output: 'static'`, no SSR adapter, dev port 4321, API base via `PUBLIC_API_URL` (`frontend/astro.config.mjs`) |
| Scripts | Only `dev`, `build`, `preview`, `check` (`astro check`) in `frontend/package.json`. No `test`, `lint`, `format`, or visual script |
| Dependencies | Runtime: `astro` only. Dev: `@astrojs/check`, `typescript`. **No Playwright, Puppeteer, Vitest, Jest, Cypress, axe, Lighthouse, Percy, Chromatic, or Storybook anywhere** (grep over `package.json`, `ci.yml`, `scripts/` = none found) |
| Lockfile | `frontend/package-lock.json` **is** committed (194 KB) — the CI comment claiming "No lockfile is committed yet" is stale (see §3, global) |
| Pages | 15 routes built: `/`, `/login`, `/register`, `/admin`, `/admin/plugins`, `/checkout/[id]`, `/pay/[slug]`, `/dashboard` + `transactions`, `payment-links`, `invoices`, `gateways`, `webhooks`, `api-keys`, `settings` |

### 1.2 What is actually verified today

- ✅ **Production build passes.** Run firsthand in this audit: `astro build`
  completed in ~1.25 s, **15 pages built, exit 0**. This is the *only*
  frontend gate in CI.
- ✅ **CI frontend job** (`.github/workflows/ci.yml`): Node 24 →
  `npm install` → `npm run build` → JS SDK tests (`npm --prefix sdk/js test`,
  zero-dependency `node:test`, stubbed fetch — tests the SDK, not the UI).
  Other CI jobs (Rust fmt/clippy/test, RustSec audit, Docker, Android) do not
  touch frontend rendering at all.
- ❌ **`astro check` is not gated and currently fails.** Run firsthand:
  **31 errors, 0 warnings, 28 hints across 17 files** (e.g. `register.astro:226`
  `unknown` not assignable to `string`; `register.astro:203` `HTMLElement` vs
  `HTMLFormElement`). CI never runs it, so type errors in page scripts are
  invisible.
- ❌ **No DOM, CSS, accessibility, contrast, or responsive assertion of any
  kind.** Nothing checks that a built page renders without horizontal
  overflow, that a class exists, that a token is used, or that focus/contrast
  rules survive an edit.
- ❌ **No screenshot/visual baseline exists** — and per the standing rule,
  none may be committed. `git ls-files | grep -iE '\.(png|jpe?g|webp)$|snapshot|__screenshots__'`
  returns nothing today: compliant, but by absence, not by guardrail.
  Root `.gitignore` covers `frontend/dist/` and `node_modules/` but has **no
  rule blocking `*.png` / screenshot / snapshot paths**, so an accidental
  commit of a visual artifact is currently possible.
- ⚠️ **Client-rendered content is untestable by static checks alone.** All
  data is fetched client-side from the Rust API (`lib/api.ts`, 929 lines);
  most dashboard/admin/checkout DOM appears only after JS + API responses.
  A build-only gate therefore verifies the shell, not the rendered UI.

### 1.3 Responsive behaviour — evidence in code

There is **no breakpoint system**. The entire codebase contains exactly
**three width media queries**, all at the same effective threshold:

| Location | Query | Effect |
|---|---|---|
| `layouts/BaseLayout.astro:160` | `@media (max-width: 640px)` | `.container` padding 1.5rem→1rem, `h1` 1.75rem→1.5rem, table cell padding shrinks |
| `pages/dashboard/invoices.astro:259` | `@media (max-width: 640px)` | `.item-row` grid 4 cols (`1fr 90px 140px auto`) → 2 cols (`1fr 1fr`) |
| `pages/index.astro:395` | `@media (max-width: 40rem)` (= 640px) | Hero/CTA flex rows wrap/centre — same threshold, different unit (px vs rem drift) |

Plus one non-width query: `@media (prefers-reduced-motion: reduce)` in
`BaseLayout.astro:80` (Wave 23 a11y — a genuine positive).

Everything else responsive is *fluid-by-construction*, not breakpoint-driven:

- `.container { max-width: 1120px; margin-inline: auto }`; landing uses its
  own `max-width: 72rem` (= 1152px) — two different page widths.
- `.grid` / per-page grids: `repeat(auto-fit, minmax(N, 1fr))` with **N
  varying by page** — 220px (BaseLayout), 180px (dashboard index),
  200px (invoices, payment-links), 240px (api-keys), 15rem (landing).
- Tables survive narrow widths only via `.table-wrap { overflow-x: auto }`
  (used on dashboard/admin list pages) — scroll, not reflow.
- `DashboardNav`: `overflow-x: auto` + `min-width: max-content` on the list —
  the 9-link nav **scrolls horizontally on mobile, never collapses/wraps**.
- Checkout/pay are capped at `max-width: 560px` — intrinsically narrow-safe.
- `<meta name="viewport" content="width=device-width, initial-scale=1">`
  is present (BaseLayout). No `100vw`, no fixed-position layout hazards found.
- **No 768px or 1024px behaviour exists at all**: tablet and small-laptop
  rendering is whatever the fluid rules happen to produce — untested at
  768/1024/1440. No `prefers-color-scheme` query; `<meta name="color-scheme"
  content="light">` — **light-only, no dark mode** (relevant to the Wave
  151–200 dark-mode scope: it starts from zero).

---

## 2. Feasible visual-regression plan (no committed images)

Principle: replace pixel-goldens with **geometry + computed-style + token
assertions**, and use screenshots only as disposable CI diagnostics.

### 2.1 Tier A — Code-level lint (no browser, runs in seconds, gate first)

1. **Design-token drift lint** (script `frontend/scripts/token-lint.mjs`,
   zero-dependency Node, run in CI):
   ```sh
   # Fail on any hex/rgb()/hsl() literal outside the token definition file
   grep -rEn '#[0-9a-fA-F]{3,8}\b|rgba?\(' frontend/src \
     --exclude=BaseLayout.astro        # ← becomes tokens file / allowlist
   # Current baseline (this audit): 63 hex total, 48 outside BaseLayout.astro
   ```
   Ratchet, don't big-bang: commit an allowlist file with the current 48,
   lint fails on any *new* entry; each design wave must shrink the list.
   Same lint for: undefined tokens (today `var(--accent)` in
   `admin/index.astro:601` and `var(--brand-green)` in `pay/[slug].astro:179`
   are referenced but never defined — fallbacks mask the bug), duplicate
   class definitions (`.btn`, `.field` redefined per-page, §3), and inline
   `style="` count per file (baseline: dashboard/index 22, settings 11,
   webhooks 9, admin/index 6 — ratchet down only).
2. **`astro check` in CI** — start as non-blocking report, promote to gate
   once the 31 existing errors are fixed (see §4).
3. **Breakpoint lint**: every `@media` width must use a token breakpoint
   from the approved set (proposed: 640 / 768 / 1024 px, one unit only);
   today this fails on `index.astro`'s `40rem` outlier by design intent.

### 2.2 Tier B — Build + DOM/CSS assertions (headless, CI-ephemeral)

Tool: **Playwright** (`@playwright/test`, Chromium only) as a *new dev
dependency + separate CI job* — the only addition needed. Astro is static,
so CI serves `frontend/dist/` with `astro preview` / a static server; the
Rust API is stubbed at the network layer (`page.route()` fulfilling
`/api/**` with fixtures) so client-rendered dashboard/checkout DOM is
deterministic without a backend or real data.

Per route × viewport, assert (no images involved):

- `document.documentElement.scrollWidth <= window.innerWidth` (no
  horizontal page overflow; `.table-wrap`/nav internal scroll is allowed —
  assert the *scroller* is the wrapper, not `body`).
- Key landmarks exist and are visible: `header/nav`, `h1`, primary CTA,
  `.card`/table/form per route; nothing `display:none` that should show.
- Computed-style spot checks against tokens: `body` background ==
  `var(--bg)` value, `.btn` background == `--brand`, `h1` font-size steps
  down at ≤640px, `.container` max-width == approved width.
- Geometry: no two interactive elements overlap; tap targets
  (`a`, `button`, `input`, `select`) ≥ 44×44 px at 360px (today: **no
  44px rule exists anywhere** — `.btn-sm` and nav links at 0.375rem vertical
  padding will fail this until the design waves fix them; record as known
  failures, not silent skips).
- A11y: inject `axe-core` (dev-only) — fail on critical/serious; assert
  `:focus-visible` outline present, skip-link present, contrast of the
  token pairs (brand-on-white etc.) computed from the token file ≥ WCAG AA.
  (Wave 23 already darkened `--brand` to `#0b7a55` ≈ 5:1 — lock that in.)
- States: empty / loading / error / populated for each data page, driven
  by the API fixtures — this is where checkout and dashboard actually break
  visually, and static HTML checks can never see it.

### 2.3 Tier C — Screenshots as disposable diagnostics ONLY

- On a Tier B failure, Playwright may capture a screenshot **inside the CI
  run** and upload it as a workflow artifact with **retention-days: 7**,
  artifact name `visual-debug-<run-id>`; it is never committed, never a
  baseline, and the job log prints the artifact link for the reviewer.
- Locally, engineers may screenshot freely — add `.gitignore` rules
  (`**/__screenshots__/`, `**/*.snapshot.png`, `test-results/`,
  `playwright-report/`) in Wave 152 so the standing rule is enforced by
  tooling, not memory. A CI guard step fails the build if any
  `*.png|*.jpg|*.webp` (outside pre-existing functional assets) appears in
  `git diff --name-only` of a PR.
- **No Percy/Chromatic/external visual SaaS**: it would upload rendered
  images off-machine and create hosted baselines — contrary to the rule's
  spirit and unnecessary given Tier B geometry assertions.

### 2.4 Breakpoint matrix

Viewports: **360** (small Android — primary BD market device class),
**768** (tablet), **1024** (small laptop), **1440** (desktop).
Routes: all 15. Combinations: **60 route×viewport cells**.

| Viewport | Must hold in every cell |
|---|---|
| 360px | No body overflow; nav scrolls internally or collapses (design decision owed); tables contained in `.table-wrap`; checkout/pay card full-bleed-safe; tap targets ≥44px; `h1` at small size |
| 768px | Grids settle at 2 cols where `minmax` implies it; invoices `.item-row` in its >640px 4-col form (today it flips at 640 — verify intent); no orphaned single-column stretch to 1120px |
| 1024px | Container capped, not edge-to-edge; dashboard grids 3–4 cols; nav fully visible without scroll |
| 1440px | Same cap as 1024 (no ultra-wide drift); landing 72rem vs app 1120px discrepancy either resolved or explicitly asserted as intentional |

Run the full 60-cell matrix on PRs touching `frontend/src/**`; run a
15-cell smoke (1440px, all routes) on every frontend PR. Estimated CI cost:
Chromium-only Playwright against static files — minutes, no external
services, no secrets.

---

## 3. Design-vs-code drift checklist (per page)

Token baseline: the *only* token definitions live in `BaseLayout.astro`
`:root` (17 tokens: `--bg --surface --ink --muted --brand --brand-dark
--brand-soft --border --danger --danger-soft --warning --warning-soft
--radius --radius-sm --shadow --font --font-mono`). There is **no token
file** — tokens, base styles, and components are one inline `<style>` in
the layout. Hex audit: **63 literals total; 48 outside `BaseLayout.astro`**,
concentrated in exactly the three pages that ignore the token system:

| Page | Drift findings |
|---|---|
| `index.astro` (landing) | ❌ Own palette: purple `#6d28d9` primary (×3) vs brand green `--brand #0b7a55` everywhere else; greys `#e5e7eb/#d1d5db/#f9fafb` ≠ `--border/--bg`. ❌ Redefines `.btn` (scoped) shadowing BaseLayout `.btn` with different padding/radius. ❌ Own width `72rem` ≠ `.container 1120px`. ❌ Only rem breakpoint (`40rem`). ✅ Fluid hero `clamp(2.25rem, 6vw, 3.75rem)`, auto-fit grids |
| `login.astro` | ❌ Fully off-token: blue `#2563eb/#1d4ed8` primary, `#e2e8f0/#cbd5e1/#64748b/#b91c1c` — 14 hex, zero brand tokens; redefines `.field`/inputs instead of BaseLayout `.input/.label`. Focus ring blue ≠ `--brand`. Colour identity: a user would not recognise this as the same product as register |
| `register.astro` | ❌ Third palette: teal `#0f766e/#115e59` primary — 14 hex, again token-free; bare `input`/`button` element selectors re-implement `.input/.btn`. Login (blue) vs register (teal) vs app (green) = three auth identities |
| `dashboard/index.astro` | ⚠️ Mostly token-based, but: 22 inline `style="` (worst file), JS sets SVG `fill '#0b7a55'` hard-coded (`:620`), stat grid `minmax(180px,1fr)` ≠ shared 220px, inline `min-width:220px` select, TOTP input inline `max-width:200px` |
| `dashboard/transactions.astro` | ✅ Token-based, `.table-wrap` used. ⚠️ One JS fallback hex `var(--danger, #b3261e)` — fallback value ≠ actual `--danger #d92d20` (stale copy) |
| `dashboard/payment-links.astro` | ✅ Token-based. ⚠️ Grid `minmax(200px,1fr)`; modal `max-width:320px` ad hoc |
| `dashboard/invoices.astro` | ✅ Token-based; ✅ only dashboard page with its own breakpoint (640px `.item-row` 4→2 col). ⚠️ Totals `max-width:360px`, item columns `90px/140px` magic numbers |
| `dashboard/gateways.astro` | ✅ Token-based, flex-wrap rows. ⚠️ No own breakpoint — wide gateway cards rely purely on wrap; untested at 360px |
| `dashboard/webhooks.astro` | ⚠️ Token-based but 9 inline styles; `max-width:22rem` ad hoc; three `.table-wrap` tables — overflow behaviour at 360px unverified |
| `dashboard/api-keys.astro` | ✅ Token-based. ⚠️ Grid `minmax(240px,1fr)` — fourth distinct grid floor; own `overflow-x:auto` block duplicating `.table-wrap` |
| `dashboard/settings.astro` | ❌ **No `<style>` block at all** — layout done with 11 inline styles (`max-width:12/16/10/14rem`, `min-width:220px`); brand-colour input defaults to literal `#0b7a55`. Least design-system-compliant dashboard page |
| `admin/index.astro` | ⚠️ Token-based shell, but chart bars use undefined `var(--accent, #2563eb)` — admin accent is an undeclared blue fallback; 6 inline styles incl. JS-built bar widths; 5 `.table-wrap` tables unverified at 360px |
| `admin/plugins.astro` | ✅ Token-based, flex-wrap. ⚠️ No breakpoint; dense plugin cards rely on wrap only |
| `checkout/[id].astro` | ✅ Best token citizen of the public pages (`var(--brand/--border/--surface/--muted)` throughout), 560px cap, `overflow-wrap:anywhere` on amounts/names. ⚠️ Fallback hex in `var(--brand, #0b7a55)` (correct today, will silently stale); `#fff` literal |
| `pay/[slug].astro` | ✅ Token-based, 560px cap, logo `max-width:220px`. ❌ Undefined token chain `var(--brand, var(--brand-green, #0b7a55))` — `--brand-green` exists nowhere; works only by double fallback |
| `components/DashboardNav.astro` | ✅ Fully token-based, custom scrollbar, `aria-current` correct. ❌ No mobile behaviour: horizontal scroll at all widths < ~900px, no collapse; lang-toggle uses inline style. Nav is the single biggest 360px risk in the app |
| `layouts/BaseLayout.astro` | ✅ Token source, skip-link, `:focus-visible`, reduced-motion, `.table-wrap`, AA-fixed `--brand`. ⚠️ Tokens + components + layout fused in one file (any token extraction in Waves 152+ touches every page); `.btn-danger:hover #b42318` is a hex outside `:root`; `color-scheme: light` only — no dark tokens exist |

**Global drift summary:** (1) three competing brand colours — green
`#0b7a55` (app), blue `#2563eb` (login/admin-chart), teal `#0f766e`
(register), plus purple `#6d28d9` (landing); (2) two page widths
(1120px / 72rem); (3) four grid floors (180/200/220/240px); (4) two
breakpoint units (px / rem) for one threshold; (5) CI comment claims no
lockfile while `package-lock.json` is committed, and CI uses
`npm install` instead of reproducible `npm ci`.

---

## 4. Exact checks to gate Waves 152–200 on

A wave does **not** close until its gate column passes. "Gate" = CI job
fails → merge blocked. Ratchets only ever move one way (down for drift
counts, up for coverage).

### Gate 0 — Wave 152 entry (infrastructure, before any redesign lands)

1. `.gitignore` + CI guard: PR diff containing any new
   `*.png|*.jpg|*.jpeg|*.webp|*.snapshot.*` under `frontend/` (functional
   assets allowlisted by path) **fails**. `test-results/`,
   `playwright-report/`, `**/__screenshots__/` ignored.
2. Playwright job exists in `ci.yml` (Chromium, static `dist/` served,
   API stubbed via fixtures) and runs the 15-route × 1440px smoke with
   the no-body-overflow + landmark assertions green.
3. `token-lint.mjs` exists with the 48-hex allowlist frozen at the
   §2.1 baseline; lint passes at baseline and fails on any addition
   (verified by a deliberate test commit in the wave's PR, then reverted).
4. CI switches frontend install to `npm ci`; stale lockfile comment fixed.

### Gate 1 — Every frontend PR, Waves 152–200 (always on)

5. `npm run build` green (existing) **and** `npm run check` reported;
   from Wave 160: `astro check` is blocking at 0 errors.
6. Full 60-cell matrix (15 routes × 360/768/1024/1440) green on:
   no body horizontal overflow; landmarks visible; computed
   `body`/`.btn`/`h1` styles equal token values; internal scrollers
   (`.table-wrap`, nav) — not `body` — own any horizontal scroll.
7. Token lint: hex/rgb/hsl outside the token file = allowlist or fewer;
   zero references to undefined CSS variables; zero new inline `style="`
   attributes (per-file counts ≤ §2.1 baseline).
8. Axe: 0 critical, 0 serious on every route in populated + empty states;
   token-pair contrast ≥ 4.5:1 (normal text) computed from the token file.
9. Screenshot artifacts, if produced on failure, are CI artifacts with
   `retention-days ≤ 7` and the PR contains **no** image files (Gate 0.1
   re-checked).

### Gate 2 — Per-design-wave exit (in addition to Gate 1)

10. **Wave touching tokens/design system:** token file is the single
    definition site; hex outside it = **0** for every page that wave
    claims as migrated; breakpoint set is exactly {640, 768, 1024} in
    one unit, defined as tokens, and the matrix gains explicit
    per-breakpoint layout assertions (e.g. nav mode, grid column count).
11. **Wave touching landing/auth:** `index/login/register` use `--brand`
    (no `#6d28d9/#2563eb/#0f766e` literals remain), share `.btn/.input/
    .field` from the system (no page redefinitions), and pass the matrix
    in both logged-out states.
12. **Wave touching dashboard/nav:** at 360px the nav matches the
    approved design (collapse or scroll — asserted explicitly, not
    incidentally); `settings.astro` has a `<style>`/class implementation
    with inline-style count = 0; dashboard/index inline styles = 0.
13. **Wave touching checkout/pay:** 360px + 768px populated/empty/error/
    expired states asserted; amount/merchant text never overflows
    (`overflow-wrap` verified by long-string fixture, incl. Bangla text);
    undefined-token chain in `pay/[slug].astro` removed.
14. **Dark-mode wave:** `prefers-color-scheme: dark` (and/or the approved
    toggle) matrix run doubles to 120 cells in dark mode; every token
    has a dark value defined in the token file; `color-scheme` meta
    updated; contrast gate (8) passes in both schemes.
15. **Android-adjacent / responsive wave:** 360px is the primary cell —
    tap targets ≥ 44×44px on all interactive elements (no known-failure
    exemptions left), and `prefers-reduced-motion` assertion (transitions
    ≈ 0) passes.

### Gate 3 — v0.0.3 design release (Wave 200)

16. All Gate 1 + applicable Gate 2 checks green on the release commit,
    verified twice at job/step level (per the project's CI-verdict rule).
17. Drift ledger at zero: 0 hex outside token file, 0 undefined tokens,
    0 inline styles in `frontend/src`, 0 page-level `.btn/.field/.input`
    redefinitions, one page width, one breakpoint set.
18. Release notes state the honest limit: visual verification is
    assertion-based (geometry/computed-style/a11y) with screenshots used
    only as ephemeral CI diagnostics — no pixel-golden suite exists,
    by design and by the user's standing rule.

---

*Method note: all counts in this file are reproducible with the grep
commands in §2.1 against the tree as of 2026-11-26; build/check results
were run, not inferred. No source files were edited for this audit.*
