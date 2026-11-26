# MiraclePay — Design Lead Review #1 (Wave 165)

**Role:** Design Lead, 10-agent design engineering team
**Review of:** Waves 152–164 as they stand in the working tree at
`~/workspace/miraclepay` — `DESIGN.md`, `frontend/src/styles/tokens.css`,
`frontend/src/layouts/BaseLayout.astro`, `frontend/src/lib/ui.ts`,
`frontend/src/components/{Icon,PageHeader,StatCard,EmptyState,DashboardNav}.astro`,
`frontend/src/pages/index.astro`, `login.astro`, `register.astro`,
`forgot-password.astro`, plus spot-checks of the not-yet-redesigned
dashboard / admin / checkout / pay pages and Android resources.
**Method:** source review only. No source files were edited; the only
file written is this review.
**Baseline:** my Wave 151 audit (`design-audit/01-design-lead.md`) and
the 8 principles + per-surface "done" definitions in `DESIGN.md`.

> **Process caveat, stated once and plainly:** `WAVES.md` formally logs
> the design program only through Waves 152–158 (CI run `45807726561`,
> commit `db88433`, verified). The 159–164 work reviewed below —
> landing, auth shell, `DashboardNav` app shell, onboarding — exists in
> the working tree (15 modified files + untracked
> `forgot-password.astro` at time of review), not yet in `git log` and
> with no wave entries or CI verdicts of its own. This review judges
> the code as found, which is what the user would get; but Waves
> 159–164 should be committed, logged and CI-verified before Wave 166
> starts, or Review #2 will be auditing a moving target again.

---

## 1. Verdict

**The foundation is real, and it is the best design work this product
has had. The adoption is not.**

In Wave 151 I said "the dashboard is a design system; everything
around it is four hand-made brochures." That sentence is now half
dead: the brochures have been rebuilt on the system. Landing, login,
register and forgot-password genuinely share one primary, one card,
one button, one focus ring. The token architecture is correct in
shape — three layers, dark mode re-pointing only the semantic layer,
a drift lint ratcheted to 0 and actually passing
(`node frontend/scripts/check-tokens.mjs` → `hex literals outside
tokens.css: 0 (baseline 0)`).

But a second, quieter version of the old problem has appeared:
**the system now exists in two places — the library, and the pages
that still hand-roll what the library already provides.**
`PageHeader`, `StatCard` and `EmptyState` have **zero usages in any
page** (grep over `frontend/src/pages/`). The `lib/ui.ts` helpers
(`statusBadge`, `toast`, `withBusy`, `skeletonAfter`, `confirmAction`,
`fieldError`, `copyText`) are used by exactly two pages
(`dashboard/invoices.astro`, `dashboard/transactions.astro`, both for
`statusBadge` only); `dashboard/index.astro`, `dashboard/webhooks.astro`
and `dashboard/api-keys.astro` each still define their **own local
`statusBadge`**, checkout defines its own `badgeClass`
(`pages/checkout/[id].astro:478`), admin builds badges inline
(`pages/admin/index.astro:424`), and `dashboard/payment-links.astro:378`
defines a private `copyText` that shadows the shared one. Landing
doesn't use `StatCard` for its stats band; no page uses `EmptyState`;
every dashboard list page still renders `<p class="empty">`.

So: **tokens — adopted. Components and state patterns — published,
not adopted.** That is exactly the failure mode Principle 2 was
written against, one layer up. Waves 166–180 must be an adoption
program, not a new-component program. Do not build anything new that
already exists in `lib/ui.ts` or `components/`.

**Score against the Wave 151 "done" definitions:** Landing ≈ 70%,
Auth ≈ 75%, Dashboard shell ≈ 50%, Dashboard pages ≈ 20%,
Checkout/Pay ≈ 0% (not yet scheduled — Waves 181–188, see §4.1),
Admin ≈ 10%, Android ≈ 0% (not yet scheduled — Waves 189–194).

---

## 2. What landed well — keep, do not relitigate

1. **One colour, enforced, not just declared (Principle 1 — largely
   won on web).** The four primaries are gone from the redesigned
   surfaces: landing violet `#6d28d9`, login blue `#2563eb` and
   register teal `#0f766e` no longer exist as page palettes; the
   drift lint is at **0** with the baseline ratcheted to 0 in
   `frontend/scripts/check-tokens.mjs`. This was finding #1 in 151
   and it is genuinely closed for landing + auth.
2. **Token architecture is the right shape.** `tokens.css` delivers
   what `design-audit/03` specified: primitives (`--p-*`) → semantics
   (`--color-*`, `--space-*`, 10-step `--text-*`, radius / shadow /
   motion / z scales) → legacy aliases; 4px spacing scale; named
   breakpoint constants (480/640/900/1120); `--target-min: 2.75rem`.
   The Wave 151/23 contrast debts are paid in the tokens themselves:
   `--color-danger-text: #b42318` (6.05:1 on danger-soft),
   `--color-border-strong` for control boundaries (WCAG 1.4.11), a
   visible `--color-focus` ring replacing the 1.12:1 outline.
3. **Dark mode exists and is architecturally cheap** — exactly as
   promised, it re-points only the semantic layer
   (`@media (prefers-color-scheme: dark)` + `:root[data-theme="dark"]`),
   `BaseLayout` now sends `color-scheme: light dark` instead of
   pinning `light`, and the dark brand (`#34c98e` on dark) is a
   considered choice, not an inversion. (Limits in §3, P8.)
4. **`BaseLayout` is now a real system file.** Beyond tokens it owns
   the patterns Principle 7 demanded, as global classes ready to use:
   `.toast` + `#toast-region`, `.skeleton` (with reduced-motion
   stilling), native `dialog.mp-dialog`, `.steps`, `.copy-field`,
   `.sticky-summary`, `.field-error`, `.table .num` (right-aligned
   tabular money columns), `.badge-info` completing the badge set.
   Whoever builds Waves 181–188 will find checkout's toolkit already
   sitting here — that was good sequencing.
5. **Auth is finally one page pattern (Waves 161–163).** Login,
   register and forgot-password share `.auth-wrap` / `.auth-card` /
   `.auth-brand` / `.auth-sub` / `.auth-switch` from `BaseLayout`,
   one width (`--container-auth: 26rem`), one shadow, system
   `.input` / `.label` / `.btn` / `.alert-*` throughout, no
   bare-element styling. The Wave 151 complaint "blue login, teal
   register, neither uses the system" is closed. Forgot-password
   (Wave 166 scope, already drafted) correctly reuses the same shell
   for both its steps instead of inventing a fifth auth look.
6. **Landing is a different product from the one I audited.** The
   hero sentence repeated 4× is gone; the single claim
   ("Payments that verify themselves") is carried by a concrete
   SMS-verification vignette; the ৳0 / 0% pricing offer is a pricing
   card with a display-size price instead of buried prose; gateways
   are grouped by region; features use the new `Icon` set (24 inline
   SVGs, `currentColor`, no emoji-as-icons); the eyebrow now reads
   v0.0.2, not v0.0.1; a real `public/favicon.svg` logo mark exists
   and is reused as wordmark icon on landing, auth and dashboard.
7. **`DashboardNav` is an app shell, not a pill strip.** Sticky top
   bar with wordmark, account email, language toggle and **log out**
   (the control whose total absence I flagged as a trust problem in
   151 now exists and calls the real `logout()`), grouped navigation
   (Operate / Configure / Admin), `aria-current` highlighting derived
   from the path, admin items hidden until `/auth/me` confirms
   `is_admin` (fail-closed), and the nav is now inside the container
   measure instead of stretching edge-to-edge above a 1120px column.
8. **`lib/ui.ts` is well-built, whatever its adoption problem.**
   `statusBadge` covers the full Wave 151 vocabulary (including
   refunded / partially_refunded / disputed-adjacent states, with
   text labels so colour is never the only signal), `skeletonAfter`
   implements the specified 150ms show-delay, `confirmAction` uses a
   native `<dialog>` instead of `window.confirm`, `fieldError` wires
   `aria-invalid` + `aria-describedby` correctly, `scrollToEl`
   respects reduced motion in JS as DESIGN.md required. The helpers
   deserve pages that use them.
9. **Onboarding (Wave 164) moved in the right direction:** the
   dashboard checklist items are now actionable links/actions rather
   than a passive progress display
   (`pages/dashboard/index.astro:517` comment + implementation), and
   the dashboard chart now reads `--color-chart-1` from tokens
   (`index.astro:641`) instead of the hard-coded `#0b7a55` in JS I
   cited in 151.

---

## 3. What still violates the 8 principles

Ordered by principle, with file evidence. Severity: **P0** = user
harm / trust, **P1** = principle broken on a shipped surface,
**P2** = drift risk / polish debt.

### Principle 1 — One colour

- **[P2] Violet survived as a *filled* interactive-adjacent surface,
  not just an accent.** Landing `.step-number` is a filled
  `--color-accent` disc with white text (`pages/index.astro:181`),
  and the eyebrow is accent-coloured (`index.astro:162`). DESIGN.md
  permits violet as "non-interactive marketing accent" — a numbered
  step disc is arguably that, but it is the second-strongest colour
  signal on the page after the green CTA, and in dark mode
  `--color-accent` flips to light violet `#a78bfa` while the disc
  keeps `color: var(--p-white)` — white on `#a78bfa` is not AA for
  anything. Either demote step numbers to brand-soft/green (my
  preference) or give the disc an on-accent token pair like
  `--color-on-brand`.
- **[P2] Pages reach into the primitive layer.** Landing uses
  `var(--p-white)` directly (`index.astro:181`); `BaseLayout`'s
  `.btn-danger` uses `var(--p-white)`. Primitives are supposed to be
  consumed only by the semantic layer — that is the whole point of
  three layers. The hex lint cannot see this (it matches `#hex`,
  not `var(--p-*)`). Extend the lint (see §4) and add
  `--color-on-danger`.
- **[P1, unchanged from 151] Android still ships the rejected
  green.** `android/.../values/themes.xml:4,7` still sets
  `colorPrimary` / `colorSecondary` to `#0E9F6E` (3.39:1) — the exact
  value the web retired for failing contrast. Scheduled for Wave
  189, but it means Principle 1's "everywhere" is still false today.

### Principle 2 — Tokens → components → pages

- **[P1] The component library is unused.** `PageHeader`,
  `StatCard`, `EmptyState`: **0 page usages.** Pages hand-write
  `.page-header` markup, `.card.stat` markup and `<p class="empty">`
  instead. A component nobody consumes is not a system, it's a
  museum. (Fix is adoption in 169–179, not new components.)
- **[P1] Shared JS patterns are re-implemented per page.** Local
  `statusBadge` in `dashboard/index.astro:363`,
  `dashboard/webhooks.astro:451`, `dashboard/api-keys.astro:445ff`;
  local `badgeClass` in `checkout/[id].astro:478`; local `copyText`
  in `dashboard/payment-links.astro:378`; login / register /
  forgot-password each hand-roll "disable button + swap text +
  restore" instead of `withBusy`, and show errors only as a top
  banner instead of `fieldError`. Five homes for success feedback
  was the 151 complaint; `toast()` currently has **zero page
  callers**.
- **[P1] Inline styles did not move: 51, exactly the 151 count.**
  `grep 'style='` over `frontend/src` = 51, distributed identically
  to the audit (dashboard/index ×22, settings ×11, webhooks ×9,
  admin ×6, pay ×1, checkout ×1) plus one new one in
  `forgot-password.astro` (the `<hr style="…">` separator — a new
  page should not be adding to this ledger). The redesigned pages
  are clean; the un-redesigned ones are untouched. Expected at this
  checkpoint, but the number must be quoted honestly at Review #2:
  the win so far is "no new drift on new surfaces", not "drift
  removed".
- **[P2] `tokens.css` breaks its own "only primitives hold hex"
  rule.** `--color-border-strong: #7d879c` is a raw hex in the
  semantic layer (`tokens.css:31`), and both dark blocks hard-code
  `#3a1512`, `#342605`, `#10264d` for the dark soft surfaces
  (`tokens.css:106–108, 124–126`). Promote all five to named
  primitives (`--p-gray-*`-style) so the file's header comment is
  true and the lint's exemption is honest.
- **[P2] The dark theme is specified twice and has already drifted
  internally.** The `@media (prefers-color-scheme: dark)` block and
  the `:root[data-theme="dark"]` block duplicate ~20 assignments;
  the media-query block also re-points `--shadow-*`, the
  `data-theme` block does not. The first future token edit *will*
  update one and miss the other. Collapse to one definition (e.g.
  custom-media-style duplication removed by defining dark values
  once under a shared selector list).

### Principle 3 — One shell per audience

- **[P1] There is still no Public shell — there are two hand-rolled
  approximations.** Landing hand-builds its own header/footer in
  page markup (`index.astro`, `.site-header` / `.site-footer` in its
  private `<style>`); auth pages have *no* header, footer or home
  link at all — `.auth-brand` is a `<p>`, not a link, so a user on
  /login cannot navigate anywhere except the other auth pages. The
  "shared auth shell" is a CSS pattern in `BaseLayout`, not a
  component: each auth page re-types the same brand block. Extract
  `PublicHeader`/`PublicFooter` + an `AuthCard` component in
  166–168 and consume them; until then Principle 3's check ("no page
  hand-rolls a header or footer") fails on the flagship page.
- **[P1] The dashboard shell lacks the one context it most needs:
  the merchant.** `DashboardNav`'s shell bar carries wordmark,
  email, language, logout — but no merchant switcher. The switcher
  still exists only on `dashboard/index.astro:18–20`; every other
  page silently resolves the merchant from `localStorage`
  (`mp_merchant` / `mp_merchant_id`, see gateways / api-keys
  resolvers). That is verbatim the 151 finding #3 sub-complaint,
  carried into the new shell. Wave 169's shell work is not done
  until the switcher lives in the shell, on every dashboard page,
  showing the active merchant's name.
- **[P2] Shell controls use the dense-table exception as a size.**
  Logout and the language toggle are `.btn-sm` (min-height 2.25rem
  = 36px). `BaseLayout` documents `.btn-sm` as "the dense-table
  exception" to the 44px floor; primary shell chrome is not a dense
  table. Give shell buttons their own ≥44px treatment or accept
  `.btn` sizing.
- **[P2] Admin is in the shell, but not marked as a mode.** The 151
  "done" definition asked for one deliberate differentiator (Admin
  context badge/banner) so an operator knows they are acting
  platform-wide. `DashboardNav` shows the Admin group label in the
  same style as Operate/Configure, and `admin/index.astro:26` still
  displays the stale string **"MiraclePay v0.0.1"** in visible copy
  (see also P-version note under Principle 8/process).
- **[P2] Plugins visibility is inconsistent with admin-hiding.**
  In `DashboardNav`, `Admin` is `adminOnly` but `Plugins`
  (`/admin/plugins`, group Configure) is not — non-admin merchants
  are shown a link into an admin surface. Either gate it or move it;
  don't let the new shell teach a wrong permission model.

### Principle 4 — Money pages are the design ceiling

- **[P0, unchanged, scheduled 181–188] Checkout/pay remain the
  weakest surfaces in the product, exactly as audited.** Neither
  page renders the merchant's receiving wallet number (grep for a
  rendered recipient/`merchant_number` on either page: nothing);
  checkout still silently uses the first gateway, pay still uses a
  bare `<select>`; neither polls (the only `setInterval` in the
  frontend is the dashboard's 30s refresh); both still carry the
  151-era markup `class="btn btn-small" style="float:right"` on
  their language toggles (`checkout/[id].astro:35`,
  `pay/[slug].astro:24`) — cosmetically rescued only because
  `BaseLayout` now aliases `.btn-small` to `.btn-sm`.
- **[P0, regression risk introduced by the token migration]
  Merchant branding on the money pages is now half-wired to a
  legacy alias.** Both pages still do
  `documentElement.style.setProperty('--brand', brandColor)` plus
  an inline `submitButton.style.background = brandColor`
  (`checkout/[id].astro:611ff`, `pay/[slug].astro:448ff`). Since the
  rebuild, the system's components consume `--color-brand`, not
  `--brand`; `--brand` survives only as a legacy alias *defined as*
  `var(--color-brand)` — setting it does not flow back into
  `--color-brand`. The pages' own private styles still read
  `var(--brand)` / `var(--brand-dark)` / `var(--brand-soft)`, so
  branding currently "works" only inside those private styles +
  the one inline button override, while every shared component on
  the page (badges, focus rings, links) stays MiraclePay green.
  This is more fragile than the 151 state, not less. Wave 187's
  contrast gate must be preceded by re-pointing branding at
  `--color-brand` + derived tokens (Principle 6).

### Principle 5 — Bangla is first-class

- **[P1] The redesigned surfaces are the *least* translated ones.**
  `data-i18n` counts today: landing **0**, forgot-password **0**,
  login 4, register 4, dashboard pages 0 (nav keys only, via
  `DashboardNav`), admin 0 — versus pay 16 / checkout 15 from the
  old program. Landing Wave 167 exists, but note what it must fix
  beyond adding keys:
- **[P1] There is no language toggle on landing or any auth page.**
  The toggle lives only in the dashboard shell and (in legacy form)
  on pay/checkout. A Bangla-speaking merchant still cannot switch
  language *before creating an account* — the precise 151 failure,
  preserved through the auth redesign. The Public shell extraction
  (§3/P3) must carry the standard toggle.
- **[P1] The Bengali fonts are named but never loaded.** The stack
  lists `"Hind Siliguri", "Noto Sans Bengali"` (`tokens.css`,
  `--font-sans`) but there is no `@font-face`, no font stylesheet
  link, nothing in `public/` — so Bangla renders in whatever the
  visitor's OS falls back to, exactly what Principle 5 forbids
  ("designed Bengali type", not a hopeful stack). Self-host at
  least one Bengali face (the product is self-hosted; a Google
  Fonts dependency is the wrong default) and subset it.
- **[P2] Dictionary drift is already visible.** `i18n.ts` says
  `auth_register_heading` = "Create your MiraclePay account" and
  `auth_name` = "Name"; `register.astro` renders "Create your
  account" and "Full name" — `applyI18n()` will silently rewrite
  both on load (in English too). One source of truth: page copy
  should be generated from the dictionary, not merely annotated
  with it.
- **[P2] `:lang(bn)` rules exist but almost nothing is ever
  `lang="bn"`.** `applyI18n` sets `documentElement.lang`, which
  covers the toggle path; static Bangla (e.g. the landing footer's
  one `<span lang="bn">`) is the exception, not the pattern.

### Principle 6 — Merchant colour is a generated theme

- **[P0, unchanged] No generation, no contrast gate, anywhere.**
  No `color-mix` derivation of hover/soft/on-brand exists in the
  frontend; settings still offers a bare colour input with no
  preview and no AA warning (`dashboard/settings.astro:241,296`);
  checkout/pay apply the raw colour as in 151 (and now onto the
  wrong token, §3/P4). A merchant can still pick `#ffe066` and ship
  themselves an unreadable Pay button. This is scheduled (187) but
  it is the oldest open P0-adjacent item in the program and must
  not slip past 188.

### Principle 7 — One pattern per state

- **[P1] Patterns exist; pages don't use them (see P2 above).**
  Concretely at this checkpoint: loading is still a
  `<p class="muted">Loading…</p>` paragraph on the dashboard home
  (`index.astro`, `#page-loading`), admin ("Loading admin panel…")
  and across the list pages — 23 `Loading` strings in the tree;
  `skeletonAfter` has zero callers. Empty states are still bare
  `<p class="empty">` with no genuine / filtered / error variants —
  and the shipped `EmptyState` component itself has no icon slot
  or variant prop, so even adopting it as-is wouldn't meet the
  DESIGN.md definition yet. Destructive actions outside the
  dashboard still need auditing against `confirmAction` in
  176–179 (admin suspend / KYC decisions are the high-stakes ones).
- **[P2] `EmptyState` needs its variants before the 179 sweep,**
  or the sweep will cement a title+body paragraph as "the pattern".
  Add: icon (via `Icon`), variant (`genuine | filtered | error`),
  and an action slot that is already there via `<slot/>` — then
  adopt it everywhere in one pass.

### Principle 8 — Shared tokens across web + Android

- **[P1] Still two platforms, two systems.** Web dark mode has no
  manual control: `data-theme` support exists in CSS and `Icon`
  ships `sun`/`moon`, but no toggle anywhere sets it — dark is
  OS-only, undiscoverable, and untestable for most users. Android
  remains M2, `#0E9F6E`, hard-coded strings (Waves 189–194, as
  scheduled). The cross-platform check in DESIGN.md ("Android theme
  colours diffed against web tokens") cannot pass until 189; flag
  it as the program's largest remaining block, not a footnote.
- **[P2] Version identity is still hand-typed.** Landing now says
  v0.0.2 twice (eyebrow + footer, `index.astro:60,151`) — correct
  today, wrong the day v0.0.3 tags, and admin still says v0.0.1
  (`admin/index.astro:26`). DESIGN.md's fix (single build-time
  version source) has not landed. Do it in 168 while landing is
  still open; it is a 30-minute change that otherwise becomes the
  third release in a row with a stale version on a public page.

---

## 4. Ranked fix list — Waves 166–180

Ranked by (user impact × surfaces) ÷ effort, mapped to the existing
roadmap waves. Items 1–4 are gates: later waves should not start
until they land.

| # | Fix | Waves | Principle |
|---|-----|-------|-----------|
| 1 | **Adopt, don't rebuild:** replace every local `statusBadge` / `badgeClass` / `copyText` with `lib/ui.ts`; wire `withBusy` into login/register/forgot + all form submits, `fieldError` for field-level errors, `toast()` as the single success home, `confirmAction` for suspend/KYC/revoke/delete. Grep-verifiable: 0 local re-implementations. | 166, 171–177 | 2, 7 |
| 2 | **Extract the real shells:** `PublicHeader`/`PublicFooter` + `AuthCard` components; landing and auth consume them; standard language toggle in the public header (fixes pre-login Bangla); auth brand becomes a home link. Merchant switcher moves into `DashboardNav` shell bar, visible and functional on every dashboard/admin page. | 166–169 | 3, 5 |
| 3 | **Component adoption sweep:** `PageHeader` on all 10 dashboard/admin pages, `StatCard` on dashboard home + landing stats + admin stats, `EmptyState` (after adding icon + genuine/filtered/error variants) on every list page; delete the hand-written equivalents. Target: each component ≥ 8 usages or it is removed from the library. | 169–179 (esp. 179) | 2, 7 |
| 4 | **Kill the inline-style ledger on dashboard/admin:** 48 of the 51 `style=` attributes live in dashboard/index, settings, webhooks and admin. Add the missing composition utilities to `BaseLayout` (`.row-between`, `.list-reset`, spacing helpers) and migrate page by page with 170–177. Gate: `grep 'style='` ≤ 2 (pay/checkout legacy toggles, removed in 181–184) by Review #2. Also remove the new one in `forgot-password.astro` now (use a token-spaced divider class). | 170–177 | 2 |
| 5 | **Auth states completed (Wave 166 as roadmapped):** TOTP step as a designed step (use `.steps`), not an error-triggered reveal driven by substring-matching `totp_required` in the error message; field-level errors via `fieldError`; forgot-password gets `data-i18n` + dictionary-generated copy; fix the `i18n.ts` ↔ register copy drift by rendering auth headings/labels from the dictionary. | 166 | 5, 7 |
| 6 | **Landing Bangla + version source (167–168):** full `data-i18n` coverage of landing; single build-time version constant consumed by landing eyebrow/footer **and** admin header (deleting the visible "v0.0.1"); self-hosted Bengali webfont actually loaded; gateway chips get the defined identity treatment (mark/colour per gateway) or the 151 "done" line is formally dropped — do not leave it silently unmet a second review running. | 167–168 | 5, 1 |
| 7 | **Loading = skeleton, everywhere in the app:** replace all "Loading…" paragraphs on dashboard/admin with `skeletonAfter` (150ms rule already implemented); keep text loading only where a skeleton cannot represent the content, and say why in the wave log. | 170–179 | 7 |
| 8 | **Token hygiene:** promote the 5 stray hex values (`#7d879c`, dark soft trio + info) to primitives; de-duplicate the two dark blocks (restore shadow parity); add `--color-on-danger` / `--color-on-accent` and remove all `var(--p-*)` use outside `tokens.css`; extend `check-tokens.mjs` to fail on `var(--p-` outside `tokens.css` and on `style="` count above a ratcheting baseline — make the lint guard the *next* drift class, not only the last one. | 168 or 175 | 1, 2, 8 |
| 9 | **Shell correctness details:** shell-bar buttons back to ≥44px; Admin mode differentiator (context badge in shell when on `/admin*`); gate or relocate the Plugins nav item consistently with `adminOnly`; hide-on-mobile account email gets a non-visual equivalent (it is the only place the signed-in identity appears — on phones it currently appears nowhere). | 169, 176 | 3 |
| 10 | **Landing accent discipline:** step-number discs to brand-soft + brand-hover text (or a proper on-accent pair with a dark-mode AA check); eyebrow stays accent *text* only. Add a dark-mode screenshot/check of landing + auth to the 168 polish pass — dark mode has shipped with, as far as the log shows, no visual QA pass at all. | 168 | 1, 8 |
| 11 | **Do not let the money-page P0s age quietly:** 166–180 does not include checkout/pay by design (181–188), but two cheap pre-fixes belong in this window because the token migration made them worse: (a) re-point checkout/pay branding from `--brand` to `--color-brand` (+ alias the page-private styles), (b) replace the `float:right` / `btn-small` toggle markup with the standard toggle when the public shell lands (item 2). The full recipient-number / gateway-selection / polling build stays in 181–184 and remains the program's #1 ship-blocker for v0.0.3. | 168–169 (pre-fixes); 181–184 (build) | 4, 6 |
| 12 | **Log the work:** commit + wave-log + CI-verify 159–164 retroactively as one entry, and return to per-wave logging. A design program whose evidence trail stops at its foundation wave cannot be reviewed at 180 or 195 — this review had to reconstruct scope from the working tree. | 165 (immediate) | process |

### Explicitly *not* in 166–180 (don't pull forward, don't forget)

- Checkout/pay full redesign, recipient number, gateway selection,
  polling, receipt — Waves 181–188, including the Principle 6
  contrast gate (187). Items 11(a/b) above are stabilisers only.
- Android M3 theme, colour roles from the web seed, strings
  extraction, dark parity — Waves 189–194. Principle 8 stays red
  until then; Review #2 should say so in its headline, not its
  footnotes.
- Responsive (360→1440), motion and final WCAG sweeps — 196–199.
  Note for 198: re-run the 151 contrast checks against the *dark*
  semantic values too; they have never been audited.

---

## 5. Closing judgement

Waves 152–158 built the right system. Waves 159–164 proved it on
the surfaces a new user touches first, and proved it well — one
colour, one auth pattern, a landing page that finally sells the
actual product, and a dashboard shell with a logout button in it.
If the program stopped here, MiraclePay would already look like one
company instead of four.

It must not mistake *publishing* the system for *finishing* it.
The next failure is already visible in miniature: three components
with zero users, five copies of one badge function, fifty-one
inline styles, and a merchant switcher that stayed behind on one
page while the shell moved on. Waves 166–180 succeed if, at Review
#2, the grep counts move — components consumed, helpers consumed,
inline styles near zero, "Loading…" gone, merchant context in the
shell — and fail if they merely add a second library beside the
first. Build less. Adopt more.

— Design Lead, Wave 165 Review #1
