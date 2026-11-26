# Wave 180 — Design Review #2: QA / Visual Regression

Reviewer: QA/Visual Regression Engineer · Date: 2026-11-26 · Scope: review only, no source edits.
Repo HEAD at review: `c11dc4c` (Waves 181–188 + 165 fixes).

## Commands run firsthand

```sh
cd ~/workspace/miraclepay/frontend && npm run build
node scripts/check-tokens.mjs
grep -rn 'style="' src ; grep -rn '<style' src
git ls-files | grep -iE '\.(png|jpe?g|webp|gif)$'
grep dark-mode selectors in frontend/dist/_astro/*.css
```

## Gate checklist

| # | Gate | Verdict | Evidence |
|---|------|---------|----------|
| 1 | Production build | **PASS** | `astro build` exit 0; 16 page(s) built in 1.80s (output: static, dir `frontend/dist/`); routes incl. `/`, `/login`, `/register`, `/forgot-password`, `/admin`, `/admin/plugins`, `/checkout/checkout`, `/pay/pay`, 8 dashboard routes. |
| 2 | Design-token drift lint | **PASS** | `node scripts/check-tokens.mjs` exit 0 — `hex literals outside tokens.css: 0 (baseline 0)`. Baseline in script is 0 (Wave 199 target already met for hex drift). |
| 3 | Inline `style="` eliminated | **FAIL** | 16 occurrences remain in `src/` (see table). 13 are static markup; 3 are inside a JS template string in `admin/index.astro:599-601`. Of the static ones, most are trivially tokenisable spacing/alignment (`margin: 0`, `justify-content: space-between`). The chart bar at `admin/index.astro:601` carries a data-driven `width:${…}%` — a legitimate dynamic exception, but it is bundled with static `display/height/background` declarations that are not. |
| 4 | Page-local `<style>` blocks eliminated | **FAIL** | 12 of 16 pages still carry exactly 1 page-local `<style>` block each (14 blocks total incl. layout/component). Mitigating: no file has more than 1 block — these are single Astro scoped-style blocks, not scattered fragments — but they have not been consolidated into shared/token CSS. |
| 5 | `.gitignore` screenshot guards landed | **PASS (with gap)** | Root `.gitignore:20-26`, headed "Design program guard (Wave 165 QA)": `**/__screenshots__/`, `**/*.snapshot.*`, `test-results/`, `playwright-report/`, `frontend/screenshots/`. Gap: no generic image-extension guard, so a stray screenshot outside those paths (e.g. `frontend/foo.png`, repo-root `shot.png`) would **not** be ignored — protection is path/pattern-specific, reinforced by gate 6 rather than by ignore rules alone. No `frontend/.gitignore` exists. |
| 6 | No image files tracked in git | **PASS** | `git ls-files \| grep -iE '\.(png\|jpe?g\|webp\|gif)$'` → no matches (grep exit 1, count 0). |
| 7 | Dark-mode token blocks in built CSS | **PASS** | Source `src/styles/tokens.css:96` `@media (prefers-color-scheme: dark)` with `:root:not([data-theme="light"])`, and `:115` `:root[data-theme="dark"]`, re-pointing only the semantic `--color-*` layer (dark primitives `--p-dark-*` at line 20). Built output confirms it survives minification: `dist/_astro/BaseLayout.Cq4gaMq4.css` (22,144 B) contains 2× `prefers-color-scheme:dark` and the `:root[data-theme=dark]` block (attribute unquoted by the minifier). The second dist CSS (`index.CE0H7lpY.css`, 4,871 B, landing-page scoped styles) contains no dark block of its own and relies on the BaseLayout token layer — acceptable, since tokens cascade globally. |

**Gate tally: 5 PASS / 2 FAIL** (gates 3 and 4 — residual inline styles and page-local style blocks).

## Inline `style="` — per page (16 total)

| File | Count | Notes |
|------|------:|-------|
| `src/pages/dashboard/webhooks.astro` | 9 | Lines 35, 38, 57, 99, 134, 135, 161, 162, 190 — all `margin: 0` / `justify-content: space-between` |
| `src/pages/admin/index.astro` | 6 | Lines 82, 83, 249 static; 599–601 in JS template string (599–600 static layout, 601 data-driven chart-bar width + static paint) |
| `src/pages/forgot-password.astro` | 1 | Line 27 — `<hr>` divider; already token-based values (`var(--color-border)`, `var(--space-6)`) but still inline |
| All other 13 pages + components/layout/lib | 0 | — |

## Page-local `<style>` blocks — per page (14 total)

| Location | Blocks |
|----------|-------:|
| `src/pages/index.astro` | 1 |
| `src/pages/admin/index.astro` | 1 |
| `src/pages/admin/plugins.astro` | 1 |
| `src/pages/checkout/[id].astro` | 1 |
| `src/pages/pay/[slug].astro` | 1 |
| `src/pages/dashboard/index.astro` | 1 |
| `src/pages/dashboard/api-keys.astro` | 1 |
| `src/pages/dashboard/gateways.astro` | 1 |
| `src/pages/dashboard/invoices.astro` | 1 |
| `src/pages/dashboard/payment-links.astro` | 1 |
| `src/pages/dashboard/transactions.astro` | 1 |
| `src/pages/dashboard/webhooks.astro` | 1 |
| `src/pages/login.astro`, `register.astro`, `forgot-password.astro`, `dashboard/settings.astro` | 0 |
| Non-page: `src/layouts/BaseLayout.astro` | 1 |
| Non-page: `src/components/DashboardNav.astro` | 1 |

## Recommended follow-ups (not actioned — review only)

1. Webhooks page first: its 9 inline styles are pure spacing/alignment and map directly onto existing utility/token classes (`.row`, `.muted`, `.small` are already on those elements).
2. Split `admin/index.astro:601`: keep only the computed `width` inline; move `display/height/background:var(--color-chart-1)` into the page's existing `<style>` block or a chart class.
3. Decide the policy for Astro scoped `<style>` blocks explicitly — if one scoped block per page is the accepted pattern, record it as the gate (max 1/page, currently satisfied by all 16 pages) instead of an elimination target.
4. Broaden the screenshot guard if stray-path screenshots are a real risk, or rely on the tracked-images check (gate 6, currently 0) as the enforcement point in CI.
