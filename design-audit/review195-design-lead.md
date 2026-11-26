# MiraclePay — Design Lead Review #3 (Wave 195, final before polish)

**Role:** Design Lead, 10-agent design engineering team
**Review of:** the whole design program as shipped at HEAD `def77d8`
(Waves 151–194) — `DESIGN.md` vs `frontend/src/` (tokens.css,
BaseLayout, components, lib/ui.ts, lib/i18n.ts, landing / auth /
dashboard / admin / payer pages) and `android/` (M3 redesign,
Waves 189–194).
**Method:** review only — no source files edited; the only file
written is this review. Every claim below is grep-verifiable in the
tree; counts are stated with their scope. Prior panels:
`review165-*` (Review #1), `review180-*` (Review #2). Where a
Review #2 finding is still open at HEAD, I say so — several are,
and two of them are now release-blocking.

---

## 1. Verdict

**CONDITIONAL NO — do not ship v0.0.3 as a "design release" yet.
The polish waves (196–199) must be re-aimed from cosmetics to
adoption before Wave 200.**

Does it read as ONE product now? **At arm's length, yes; at arm's
reach, no.** The skeleton is unified — one green, one wordmark,
one shell per audience, one type system, dark mode on both
platforms, an Android app that finally looks like a sibling
instead of a stranger. That is a genuine transformation from the
four-primaries, no-shell product of Wave 151, and the team should
be credited for it.

But the moment a user *does* something — waits for a page, saves a
form, deletes a key, switches merchant, switches language, pays —
each surface still improvises its own behaviour. The design
system's second layer (components and state patterns) was **built
and then largely not used**: `toast`, `withBusy` and
`skeletonAfter` have **zero call sites** in any page; `EmptyState`,
`PageHeader` and `StatCard` have **zero usages**; the one page
that adopted the shared confirm dialog **calls it with the wrong
signature and ships a broken dialog**. Wave 151's disease was
"no system." Wave 195's disease is subtler: **a good system,
unevenly obeyed** — and uneven obedience is exactly what a user
experiences as "not one product."

The good news: nothing below requires new design. It requires
finishing the adoption of designs already approved. That is
precisely what Waves 196–199 are for — if we spend them there
instead of on new polish.

---

## 2. Principle-by-principle scorecard (DESIGN.md §Principles)

Scale: 5 = principle enforced in code, 1 = principle aspirational.

| # | Principle | Score | Verdict |
|---|-----------|-------|---------|
| 1 | One colour | **4.5 / 5** | ✅ Enforced, with two footnotes |
| 2 | Tokens → components → pages | **2.5 / 5** | ⚠️ Tokens exemplary; component layer bypassed |
| 3 | One shell per audience | **4 / 5** | ✅ Four shells, universal — merchant context missing |
| 4 | Money pages are the design ceiling | **3 / 5** | ⚠️ Checkout yes; pay is a floor below it |
| 5 | Bangla is first-class | **2 / 5** | ❌ First-class on payer pages only |
| 6 | Merchant colour is a generated theme | **2 / 5** | ❌ A gate exists; the "generated theme" does not |
| 7 | One pattern per state | **2 / 5** | ❌ Infrastructure built, adoption failed |
| 8 | Shared tokens across web + Android | **3.5 / 5** | ✅ Same seed, both dark — hand-mirrored, dark values diverge |

**Program average: 3.0 / 5.** Wave 151 baseline was ~1 / 5. The
foundation waves (152–158) and Android waves (189–194) are the
strongest work in the program; the middle waves built pages
faster than they adopted the system those pages were supposed
to be built from.

### P1 — One colour (4.5)
The win of the program. `node frontend/scripts/check-tokens.mjs`
run firsthand at HEAD: **0 hex literals outside tokens.css,
baseline 0, wired into CI** (`.github/workflows/ci.yml` runs
`npm run check:tokens` before build). The four competing
primaries of Wave 151 are gone; login blue and register teal no
longer exist. Android light primary is exactly the seed
`#0B7A55`. Violet survives only via `--color-accent` on the
landing eyebrow and step-number discs — sanctioned by the
principle, though a filled violet disc reads as a *second brand
colour* to a cold eye; keep it text-only in polish. Blue survives
only as info/chart tokens. Footnote two is under P8 (dark
primaries diverge across platforms).

### P2 — Tokens → components → pages (2.5)
Layer 1 (tokens) is the best artefact in the repo: three layers,
primitives the only hex, dark mode re-pointing semantics only,
legacy aliases contained. Layer 2 (components) is where the
program stalls:

- `EmptyState.astro`, `PageHeader.astro`, `StatCard.astro`:
  **0 usages anywhere.** Every dashboard/admin page hand-rolls
  its `<h1>` header (10 pages), its stat cards
  (`dashboard/index.astro` hand-writes the exact markup
  `StatCard` exists to render), and its empty states as bare
  `<p class="empty">` (≈15 instances, none with the component's
  title/body/action structure, none distinguishing
  genuine/filtered/error variants as P7 requires).
- The shared `statusBadge` taxonomy (`lib/ui.ts`) is imported by
  exactly **3 of the 11** badge-rendering surfaces (transactions,
  invoices, checkout). The rest re-implement it: a local
  `statusBadge()` in `dashboard/index.astro` and another in
  `webhooks.astro` (shadowing the shared name), an own map in
  `api-keys.astro`, an if/else chain in `payment-links.astro`,
  class strings in `gateways.astro` / `admin/plugins.astro`, a
  `badgeClass` in `admin/index.astro`. Worse, the local copies
  **contradict** the shared taxonomy: overview maps `cancelled`
  → failed (red) and `refunded` → failed, where the shared
  taxonomy maps them to neutral and info; overview renders the
  raw status string as the label, the shared one renders curated
  labels. The badge contradiction Review #2 was written to kill
  is still shipping on the dashboard's front page.
- The hex lint's success hides the drift it doesn't measure:
  **16 inline `style=` attributes** remain (9 in webhooks, 5+ in
  admin, 1 in forgot-password), **12 of 16 pages still carry
  page-local `<style>` blocks** (up to 150 lines on checkout),
  and the lint cannot see an **undefined token reference**:
  `BaseLayout.astro` styles the payer wordmark with
  `var(--text-md)` — a token that does not exist in tokens.css
  (the scale jumps `--text-base` → `--text-lg`). It silently
  falls back. A token system whose lint checks literals but not
  references is half a gate.

### P3 — One shell per audience (4)
All four shells exist and — this matters — are **universal
within their audience**: `DashboardNav` (sticky app bar:
wordmark, account email, language toggle, wired logout, grouped
nav, admin items hidden for non-admins) is on **10/10**
dashboard+admin pages; the auth card shell covers login,
register **and** forgot-password; the payer top bar covers both
checkout and pay; landing has its own header/footer. Wave 151's
"no header, no logout, merchant switcher on 2 of 8 pages" is
mostly closed. Two deductions:

- **The shell identifies the person but never the merchant.**
  `DashboardNav` shows the account email; the merchant switcher
  still exists on only **2 of 10 pages** (overview, settings),
  and the other eight silently act on `localStorage`
  (`mp_merchant` / `mp_merchant_id`, resolved by at least four
  slightly different code paths). A merchant with two stores can
  be looking at the wrong store's money with no on-screen
  evidence. The principle says "merchant context on every page";
  we shipped identity without context.
- The roadmap promised a sidebar/topbar shell (Wave 169); what
  shipped is topbar + pill strip only. It works, and I am not
  asking to rebuild it — but the record should stop claiming a
  sidebar that doesn't exist.

### P4 — Money pages are the design ceiling (3)
**Checkout is the ceiling, and it is genuinely good**: shared
payer shell, 3-step indicator, skeleton loading (the only page
in the product with one), copyable reference, field-level
validation (BD wallet pattern, TrxID format) via `fieldError`,
status polling (6s × 30) while processing, trust row with the
best trust sentence in the product. **Pay is not at the same
ceiling**, and payer-surface divergence is itself a consistency
failure:

- Pay has **no polling** — its flow still terminates at
  "Payment started" (Review #2 finding C-02, still open), with
  **no sender-number / TrxID capture**, so if SMS matching fails
  the payer has no in-page recovery. Checkout has the capture
  form; pay — the page a stranger is most likely to land on —
  does not.
- Pay loading is a bare text card; checkout's is a skeleton.
  One template, two loading treatments.
- Gateway models diverge: pay offers **selection** but from a
  **hard-coded list of 6 radio cards** in markup (all hinted
  "Send money"); checkout offers **no selection at all** —
  the gateway is a read-only row and submission silently uses
  `gateways[0]` (Review #2 C-03, still open).
- The Wave 151 P0 stands, honestly disclosed in WAVES.md and
  re-confirmed here: **neither page can render the merchant's
  receiving wallet number**, because the public checkout API
  doesn't expose it. Checkout's "How to pay" copy still tells
  the payer to "send the amount to the merchant using the
  payment method above" — a number the page never shows. This
  is now a **backend decision blocking a design release**, and
  Wave 200 must not paper over it.
- Neither payer page uses the `.sticky-summary` class
  BaseLayout defines for exactly this purpose; amount prominence
  is good on pay (large fixed amount, repeated on the submit
  button) and merely adequate on checkout (a `<dl>` row).

### P5 — Bangla is first-class (2)
What shipped is a **payer-surface translation**, not first-class
Bangla. The type foundation is real — Hind Siliguri / Noto Sans
Bengali in the stack, `:lang(bn)` rules (1.7 line-height, no
uppercase/letter-spacing) in tokens.css — and the payer pages
are properly bilingual (39 `data-i18n` attributes across
checkout+pay, 46 keys in `lib/i18n.ts`, EN+BN steps, errors and
trust copy). Beyond that, the cliff:

- Dashboard pages carry **0 `data-i18n` attributes**. The বাংলা
  toggle in the dashboard shell translates the nav labels and
  nothing else on the page — a toggle that changes the frame
  but not the picture is worse than no toggle; it promises a
  language the product then doesn't speak.
- Landing has **no toggle and no translations** (one footer line
  carries `lang="bn"`); forgot-password has 0 keys; admin has 0.
- **Android has no Bangla at all**: strings were extracted
  (7 → 52) but there is no `values-bn/` — Wave 193's "i18n"
  delivered the *mechanism*, not the language.
- **Numerals are never Bengali**: `formatMoney` is hard-wired
  to `en-US`; a BN payer sees Latin digits throughout,
  including amounts — the one string class a money product
  must get right per-locale.

### P6 — Merchant colour is a generated theme (2)
The principle demands three things: derivation (hover/soft/
on-brand via `color-mix`), a **≥4.5:1** contrast check, enforced
**at settings-save and at render**. Shipped: none of the three
as specified. `color-mix` appears **nowhere** in the codebase.
The settings page saves `brand_color` with format validation
only — **no contrast check at save**, no preview, no warning.
At render, `brandAccent()` gates the payer button at **≥3:1**
(the large-text floor, not the principle's 4.5:1) and then
writes the colour into `--color-brand` wholesale, recolouring
step dots, icons and small elements that were never checked at
even 3:1 (Review #2 checkout panel, still accurate). A merchant
can pick a colour that passes the gate and still produces
illegible small text on their own checkout. This is the
principle where the gap between DESIGN.md's words and the code
is widest relative to effort: the fix is small (save-time check
+ honest threshold + derive, don't reassign).

### P7 — One pattern per state (2)
The helpers are well-built. They are also, measurably, **not
the product's patterns**:

| Pattern | Designed | Shipped reality at HEAD |
|---|---|---|
| Loading | skeleton, 150ms delay | `skeletonAfter`: **0 call sites**. Text "Loading…" paragraphs on **11 of 16 pages**; real skeleton markup on exactly 1 (checkout) |
| Success | toast | `toast`: **0 direct call sites** (fires only inside `copyText`). Success = page-top `.alert-success` banners on ≥10 pages, each with its own id and lifetime |
| Busy | `withBusy` spinner+label | `withBusy`: **0 call sites**. ~25 hand-rolled `disabled = true` + label-juggling blocks, inconsistent labels ("Saving…", "Sending…", "Queuing…", none) |
| Destructive | `<dialog>` confirm | **1 adopter — and it is broken** (below). 2FA disable and other destructive actions proceed with no confirm at all |
| Empty | `EmptyState` variants | Component unused; bare `<p class="empty">` everywhere; filtered-vs-genuine distinguished in copy on transactions only |

**The broken dialog deserves its own paragraph.** Review #2
(motion panel, R1) flagged that `api-keys.astro` calls
`confirmAction({ title, body, confirmLabel, danger })` while
`lib/ui.ts` defines `confirmAction(message: string,
confirmLabel)`. **It is still broken at HEAD.** At runtime the
object is coerced into the dialog's paragraph: the one
destructive-confirm in the product — revoking an API key —
shows the user **"[object Object]"** and a generic "Confirm"
button. This survived two review cycles. It is a one-line-class
fix, and its survival is the single clearest sign that adoption
work is not being verified end-to-end. Release-blocking.

One credit: "never optimistic for money movement" held —
checkout/pay copy never claims money arrived before
verification, and submitted/processing/completed states are
honest. That part of the principle is intact.

### P8 — Shared tokens across web + Android (3.5)
Android is the program's quiet success (see §3). Both platforms
derive from the green seed, both ship light+dark, Android's M3
role set is complete (primary → inverse-surface, containers,
outline variants) in `values/` + `values-night/`, with shape
scale (8/12/16dp ≙ web radius scale) and a 4dp spacing token
file. Deductions: the tokens are **hand-mirrored, not shared** —
CSS and XML are separate hand-maintained copies with no
generation step and no lint covering the Android side, so drift
is a matter of time; and drift has already started in the one
place users compare side-by-side: **dark-mode brand green is
`#34c98e` on web (`--p-dark-brand`) but `#7ED7AE` on Android**
(`values-night md_primary`). Same seed, two derivations, no
documented rule for which is canonical.

---

## 3. What genuinely landed (credit where due)

- **Tokens + hex lint + dark semantic layer** (152–158): real,
  CI-gated, verified firsthand at HEAD. The model for how every
  other pattern should have been enforced.
- **Shells** (159–180): four audiences, four shells, no orphan
  pages; logout wired for the first time in the product's life;
  onboarding checklist made actionable.
- **Checkout** (181–188): steps, polling, field validation, copy
  fields, brand gate, trust row — the ceiling P4 asks for.
- **Android M3** (189–194): a complete redesign, not a retheme —
  full M3 role set light+night from the corrected seed (the
  pre-WCAG `#0E9F6E` is gone), M3 `textAppearance` type roles in
  layouts, status hero card driven by a state matrix, permission
  /battery banners that explain consequences, the outbox as a
  real RecyclerView list over `OutboxStore` with an empty state,
  settings rebuilt on outlined `TextInputLayout` with a working
  Up button and sticky action bar, Toasts retired for Snackbars,
  layouts fully on `@string` resources (52 strings). Judged
  against its Wave 151 baseline ("pre-design-system"), Android
  improved more than any web surface.
- **Landing** (159–168): the repeated-sentence hero, `v0.0.1`
  eyebrow and buried pricing are gone; there is a wordmark, a
  logo mark, a favicon, a verification-moment hero card, and
  ৳0/0% pricing stated plainly. (Remaining: gateway wall is
  still identical text chips — grouped by region now, but no
  brand marks — and social proof is still absent by choice of
  honesty; the stats band is the substitute.)

---

## 4. Top remaining inconsistencies, ranked

1. **Broken destructive confirm** — `confirmAction` signature
   mismatch at its only call site (`api-keys.astro` vs
   `lib/ui.ts`); the API-key revoke dialog renders
   "[object Object]". Flagged at Review #2, unfixed at HEAD.
   *Functional bug, not a style nit.*
2. **State-pattern schism** — toast/withBusy/skeletonAfter built
   but unused (0 call sites each); the product's real patterns
   remain page-top alerts (≥10 pages), text loaders (11/16
   pages) and ad-hoc disabled buttons (~25 sites). Users feel
   this on every interaction.
3. **Payer-surface divergence** — checkout polls, pay doesn't;
   checkout has a skeleton, pay has a text card; pay has gateway
   selection (hard-coded), checkout has none (silent
   `gateways[0]`); checkout captures TrxID, pay's flow ends at
   "Payment started" with no recovery path. Two pages, one job,
   two products.
4. **Badge taxonomy schism** — shared `statusBadge` on 3/11
   surfaces; local reimplementations on the overview, webhooks,
   api-keys, payment-links, gateways and both admin pages, with
   contradictory colour mappings for cancelled/refunded and raw
   vs curated labels.
5. **Component layer bypass** — `EmptyState` / `PageHeader` /
   `StatCard` at 0 usages while every page hand-rolls all three
   patterns; `Icon` reaches only landing + payer pages (the
   entire dashboard/admin is icon-free, so nav and actions are
   text-only there while marketing is icon-led).
6. **Invisible merchant context** — shell shows the user, never
   the active merchant; switcher on 2/10 pages; the rest resolve
   `localStorage` silently through divergent code paths.
7. **The Bangla cliff** — payer pages bilingual; dashboard
   toggle translates nav only; landing/forgot/admin untranslated;
   Android has no `values-bn`; numerals always Latin. The
   language experience changes at the login boundary — the
   exact "journey changes at every step" failure Wave 151
   condemned for colour, reincarnated for language.
8. **Merchant brand gate weaker than its principle** — 3:1 not
   4.5:1, render-time only, no save-time check in settings, no
   `color-mix` derivation anywhere; the gated colour recolours
   unchecked small elements via `--color-brand`.
9. **Lint blind spots** — the CI gate counts hex only: 16 inline
   styles, 12 page-local style blocks and an undefined
   `var(--text-md)` all pass. Drift has moved to where the lint
   doesn't look.
10. **Cross-platform token drift, unguarded** — dark brand
    green already differs web vs Android; tokens hand-mirrored
    with no shared source or Android-side check. Plus a smaller
    tell: dark mode on web is OS-only — the `data-theme`
    override blocks exist in tokens.css but **no UI anywhere
    can set them**; the manual theme control the tokens were
    built for was never shipped.

---

## 5. Fix list demanded before a v0.0.3 design release

Assigned to the polish waves. P0 items block Wave 200; P1 items
block calling the release a *design* release; P2 items may ship
as documented known-issues only with my written sign-off per item.

### P0 — release-blocking (Wave 196–197)
1. **Fix the `confirmAction` contract** — pick one signature
   (recommend the object form the caller already uses:
   `{title, body, confirmLabel, danger}`), update `lib/ui.ts`,
   and verify the revoke dialog end-to-end. Add every other
   destructive action to it: 2FA disable, webhook/API-key
   destructive ops, admin suspend/reject paths.
2. **Adopt the state patterns or delete them** — migrate all
   dashboard/admin pages to `toast` for success, `withBusy` for
   submit buttons, `skeletonAfter` for list loading; retire the
   page-top success alerts and "Loading…" paragraphs. If any
   helper cannot be adopted, remove it and amend DESIGN.md —
   a system with dead helpers is worse than a smaller live one.
3. **Payer parity** — pay gets: skeleton loading, status polling
   equivalent to checkout's, and a sender-number/TrxID fallback
   form on the started state. Checkout gets real gateway
   selection, or — if the API truly offers one gateway — copy
   that says so instead of a silent default. Both pages use the
   defined `.sticky-summary` for the amount on mobile.
4. **Recipient-number decision** — escalate to backend: expose
   the merchant's receiving number + per-gateway instructions
   on the public checkout API, or Wave 200 ships with the P0
   from Wave 151 still open and the release notes must say so
   in the first paragraph. Design cannot close this alone;
   leaving checkout copy that references a number the page
   never shows is not an acceptable third option.

### P1 — required for the "design release" claim (Wave 198–199)
5. **One badge taxonomy** — delete the local badge functions in
   `dashboard/index`, `webhooks`, `api-keys`, `payment-links`,
   `gateways`, `admin/index`, `admin/plugins`; all surfaces use
   `lib/ui.ts statusBadge`. Add a lint/grep gate for local
   redefinitions.
6. **Use the components** — `PageHeader` on all 10 dashboard/
   admin pages, `StatCard` on the overview, `EmptyState` (with
   genuine/filtered/error variants) for every empty list; or
   delete the components and standardise the hand-rolled markup
   into BaseLayout classes. No third state where both exist.
7. **Merchant context in the shell** — active merchant name in
   `DashboardNav` on every page, switcher in the shell (not
   per-page), one shared merchant-resolution helper replacing
   the divergent `localStorage` paths.
8. **Merchant colour per the principle** — contrast validation
   at settings-save (block or warn-and-confirm below 4.5:1,
   with live preview), render gate raised to the principle's
   threshold for text-bearing elements, hover/soft/on-brand
   derived via `color-mix` instead of wholesale `--color-brand`
   reassignment.
9. **Close the lint blind spots** — extend `check-tokens.mjs`
   (or a sibling gate) to fail on: inline `style=` outside an
   allowlist, `var(--…)` references to undefined tokens (would
   have caught `--text-md`), and local `statusBadge`
   definitions. Inline styles to zero (webhooks/admin/
   forgot-password are the residue).

### P2 — polish, or documented known-issues (Wave 199)
10. **Bangla, honestly scoped** — translate dashboard chrome
    (headers, table columns, form labels) or scope the toggle's
    promise to what it covers; add forgot-password keys; ship
    Android `values-bn` for the 52 extracted strings; Bengali
    numeral rendering for amounts when `lang = bn` on payer
    pages.
11. **One dark brand** — reconcile web `#34c98e` vs Android
    `#7ED7AE` to a single documented derivation from the seed;
    ship a manual theme toggle so `data-theme` is reachable, or
    remove the dead override blocks.
12. **Token source of truth** — generate Android `colors.xml`
    from tokens.css (or vice versa) in CI, or add a parity check
    that fails when seed/role values diverge.
13. **Landing finish** — gateway wall gets distinguishing brand
    treatment (marks or colour-coded chips, still token-driven);
    violet accent demoted to text-only.

---

## 6. Closing judgment

Wave 151 asked whether MiraclePay could look like one product.
Waves 152–194 answered the *looking* part. What Review #3 finds
is that **looking like one product and behaving like one product
are different finish lines**, and this program is at the first
one. The remaining distance is not talent or time — every P0/P1
fix above is adoption of work already designed and mostly
already built. Spend Waves 196–199 on adoption and verification
(the confirm-dialog bug surviving two reviews is the warning),
and v0.0.3 can honestly call itself the design release. Spend
them on new polish, and we will ship a prettier version of the
same inconsistency — and Review #3 will have been the review
that said so.

— Design Lead, Wave 195
