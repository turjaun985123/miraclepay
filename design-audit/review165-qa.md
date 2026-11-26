# Wave 165 — Design Review #1: QA / Visual Regression

Reviewer: QA / Visual Regression Engineer (Wave 151 plan: `design-audit/10-qa-visual.md`)
Date: 2026-11-26 · Review only — no source files edited.
Scope verified firsthand: frontend build, token lint, CI frontend job, built
`dist/` CSS/HTML, git history (last 3 commits), `.gitignore`.
Tree state at review: HEAD `db88433` ("Waves 152-158: design tokens (3-layer,
dark mode), component library + UI helpers, icon set, favicon, token drift
lint gate") **plus a large uncommitted working tree** (15 files changed,
incl. `check-tokens.mjs`, landing/login/register, nav; new untracked
`forgot-password.astro`). Results below are for the working tree unless
marked "committed" — they are not yet what CI on `main` would see.

## Gate checklist

| # | Check | Verdict | Evidence |
|---|---|---|---|
| 1 | Frontend build passes | ✅ PASS | `cd frontend && npm run build` run firsthand 2026-11-26: exit 0, `16 page(s) built in 1.64s`, output `static` → `frontend/dist/`. Caveat: 16th page is the uncommitted `forgot-password.astro`; committed HEAD builds 15 pages. |
| 2 | Token lint passes | ✅ PASS (working tree) / ⚠️ looser at HEAD | `npm run check:tokens` run firsthand: exit 0, `hex literals outside tokens.css: 0 (baseline 0)`. Caveat: the `TOTAL_BASELINE = 0` + anchor-stripping in `frontend/scripts/check-tokens.mjs` are uncommitted; committed HEAD still has `TOTAL_BASELINE = 48`, so HEAD passes only by ratchet allowance, not at zero. |
| 3 | CI frontend job runs `check:tokens` | ✅ PASS (committed) | `.github/workflows/ci.yml`, `frontend` job: step "Design-token drift lint (Wave 158)" → `run: npm run check:tokens`, ordered before "Build the frontend" (`npm run build`). Added in `db88433` (+3 lines). Not re-verified from a live CI run in this review — file-level evidence only. |
| 4 | Dist CSS contains dark-mode tokens | ✅ PASS | Fresh dist `frontend/dist/_astro/BaseLayout.C_-2BnJ3.css` (19,205 B): 1× `@media (prefers-color-scheme:dark)` with `:root:not([data-theme=light])`, 2× `data-theme` (incl. `:root[data-theme="dark"]`), 14× `--p-dark-*` incl. `--p-dark-brand:#34c98e`. Source: `frontend/src/styles/tokens.css:96-127`. Built HTML now sends `<meta name="color-scheme" content="light dark">` (was `light` only at Wave 151). |
| 5a | No screenshots/images committed | ✅ PASS | `git log --stat -3` (`db88433`, `1bb2b06`, `ca8c487`): only `.yml/.md/.json/.mjs/.astro/.ts/.css` + `frontend/public/favicon.svg` — no `.png/.jpg/.jpeg/.webp/.gif`. `git ls-files \| grep -iE '\.(png\|jpe?g\|webp\|gif)$\|__screenshots__\|snapshot'` = none tracked, repo-wide. `frontend/dist/` is git-ignored (`.gitignore:4`, confirmed via `git check-ignore -v`). |
| 5b | `.gitignore` guard against future image/screenshot commits (Wave 151 Gate 0.1) | ❌ FAIL | Root `.gitignore` covers `frontend/dist/`, `node_modules/`, `*.apk/*.aab`, `.env*` — **no** `*.png/*.jpg`, `**/__screenshots__/`, `*.snapshot.*`, `test-results/`, or `playwright-report/` rule, and no CI diff-guard step exists. Compliance in 5a is still by absence, not by guardrail — exactly the Wave 151 finding, unfixed. |
| 6 | Favicon present in dist | ✅ PASS | `frontend/dist/favicon.svg` present after fresh build (244 B, copied from committed `frontend/public/favicon.svg`, added in `db88433`); `frontend/dist/index.html` contains `<link rel="icon" href="/favicon.svg" type="image/svg+xml">` and reuses it as the header/footer wordmark `<img>`. SVG is a functional asset, not a screenshot — does not breach check 5. |

**Score: 5 PASS, 1 PASS-with-caveat (2, split as 5a/5b), 1 FAIL (5b guard).**
Nothing in checks 1–4 or 6 blocks the design programme; check 5b does
block Gate 0 closure (below).

## What Gates 1–3 (Wave 151 plan, §4) still need before v0.0.3

### Gate 0 — not yet closed (prerequisite, was due at Wave 152 entry)
- ❌ 0.1 `.gitignore` + CI image guard (check 5b above).
- ❌ 0.2 Playwright job: no `playwright`, `puppeteer`, `vitest`, or `axe-core`
  in `frontend/package.json` / `ci.yml` (grep = none); no 15-route smoke exists.
- ⚠️ 0.3 Token lint exists and now reads 0 in the working tree, but the
  zero-baseline version is **uncommitted** — commit it (with the landing/auth
  migration it depends on) or Gate 0.3 remains at the 48 baseline on `main`.
- ❌ 0.4 CI still uses `npm install` with the stale "No lockfile is committed
  yet" comment, although `frontend/package-lock.json` is committed; switch
  to `npm ci`.

### Gate 1 — every frontend PR (not yet enforceable)
- ✅ Build green (check 1) and token lint in CI (check 3) — the only two
  Gate 1 items live today.
- ❌ `astro check` is still not in CI at all (grep = none), so the Wave 151
  baseline of 31 errors is untracked; Gate 1.5 requires it reported now and
  blocking at 0 errors from Wave 160.
- ❌ 60-cell matrix (15–16 routes × 360/768/1024/1440: no body overflow,
  landmarks, computed-style = token values) — no harness exists (see 0.2).
- ⚠️ Token-lint sub-items: hex = 0 ✅ (working tree); undefined-variable
  references — spot grep for `var(--accent` / `--brand-green` now returns
  none (progress vs Wave 151), but the lint script does not check this, so
  it is unverified systematically; inline `style="` ratchet — no lint/count
  exists yet.
- ❌ Axe / contrast gate (0 critical/serious, token pairs ≥ 4.5:1, both
  colour schemes) — no tooling; dark-mode contrast is asserted nowhere
  despite dark tokens now shipping (check 4).

### Gate 2 — per-design-wave exit
- ⚠️ Tokens wave (152–158): single definition site achieved in substance
  (`tokens.css` + baseline 0 working tree), but uncommitted; breakpoint set
  {640, 768, 1024} as tokens + per-breakpoint layout assertions not evidenced
  in this review and not asserted by any test.
- ⚠️ Landing/auth wave: working-tree diff heavily rewrites `index.astro`
  (−503-line churn), `login.astro`, `register.astro` — consistent with the
  Gate 2.11 migration off `#6d28d9/#2563eb/#0f766e`, and token lint at 0
  supports it, but it is uncommitted and has no matrix/axe run behind it.
- ❌ Dashboard/nav (2.12), checkout/pay states (2.13), dark-mode 120-cell
  run (2.14), and 44×44px tap-target / reduced-motion assertions (2.15):
  all depend on the missing Playwright/axe harness — cannot pass, only be
  claimed, until Gate 0.2 lands.

### Gate 3 — v0.0.3 design release (Wave 200)
- ❌ 3.16 All Gate 1 + applicable Gate 2 checks green on the release commit,
  verified twice at job/step level — impossible until Gates 0–2 close and
  the current working tree is committed and run through CI.
- ❌ 3.17 Drift ledger at zero: hex = 0 is reached in the working tree only;
  inline styles, page-level `.btn/.field/.input` redefinitions, one page
  width, and one breakpoint set still need a counted, lint-enforced ledger.
- ❌ 3.18 Release notes must state the honest limit (assertion-based
  verification, screenshots only as ephemeral CI diagnostics, no pixel-golden
  suite by design) — not yet drafted.

**Bottom line for v0.0.3:** the token/dark-mode/favicon foundation is real
and build-verified, but the release is blocked less by design code than by
verification infrastructure: commit the working tree, close Gate 0.1/0.2/0.4,
then Gates 1–3 become checkable instead of aspirational.

*Method: every verdict above is from a command run or file read in this
review (build, lint, grep over dist CSS/HTML, `git log --stat -3`,
`git ls-files`, `git check-ignore`). No CI run was observed live; check 3
is file-level evidence. No source files were edited.*
