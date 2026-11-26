# MiraclePay — Design Lead Audit (Wave 151)

**Role:** Design Lead, 10-agent design engineering team
**Scope audited:** `frontend/src/layouts/BaseLayout.astro` (171 lines), all 15 pages under
`frontend/src/pages/**`, `frontend/src/components/DashboardNav.astro`,
`frontend/src/lib/i18n.ts` (176 lines), Android resources under `android/app/src/main/`,
and the tail of `WAVES.md` (Waves 51–150 log, v0.0.2 release record).
**Method:** source audit only. No source files were edited. Every claim below cites a
file and line so the team can verify it in seconds.
**Product state at audit:** v0.0.2 released (WAVES.md, Wave 149–150: tag `v0.0.2`,
717 Rust tests, frontend build = 15 pages). Waves 151–200 are the design program;
no backend feature work is in scope.

---

## 1. Overall visual consistency verdict

**Verdict: PARTIALLY CONSISTENT — a real design system exists in exactly one place,
and three of the highest-traffic surfaces ignore it.**

MiraclePay is not a visual free-for-all. There is a genuine, thoughtfully built
foundation, and the eight dashboard/admin pages that consume it look like one product.
But the three surfaces a new user touches first — landing, login, register — each
ship their own private palette, their own button/input/card CSS, and in the landing
page's case a *different brand colour*. A first-time visitor experiences three
different products before they ever reach the consistent fourth one (the dashboard).
Android is a fifth, older dialect of the brand.

### What is genuinely consistent (keep, build on this)

- **One layout, universally used.** All 15 pages import and render `BaseLayout`
  (e.g. `pages/dashboard/index.astro:5,9`; `pages/pay/[slug].astro:17,20`;
  `pages/index.astro:2,82`). There is no rogue standalone HTML document anywhere.
- **A real token set exists.** `BaseLayout.astro:26–48` defines `--bg`, `--surface`,
  `--ink`, `--muted`, `--brand` `#0b7a55`, `--brand-dark`, `--brand-soft`, `--border`,
  `--danger`/`--warning` + soft variants, `--radius` 12px, `--radius-sm` 8px,
  `--shadow`, `--font`, `--font-mono`. The Wave 23 comment at `BaseLayout.astro:31–32`
  even records *why* the brand green was darkened (WCAG AA, ≈5:1 on white) — this is
  design-system thinking, not accident.
- **Shared component classes are heavily reused inside the app shell.** Across the
  pages: `.card` ×59, `.input` ×61, `.label` ×71, `.alert` ×53, `.btn` ×39,
  `.table` ×26, `.stat` ×34, `.empty` ×19 (grep counts over `frontend/src/pages`).
  Dashboard and admin pages overwhelmingly compose these instead of inventing new ones.
- **One page skeleton in the app.** 10 of 15 pages use the identical wrapper
  `<main id="main" class="container stack">` (all 8 dashboard pages + both admin
  pages), with `.container` capped at 1120px (`BaseLayout.astro:91`).
- **Accessibility basics are systemic, not per-page.** Skip link, `:focus-visible`
  ring, `prefers-reduced-motion` block and the single 640px breakpoint all live once
  in `BaseLayout.astro:62–89,160–167`; Wave 132 added `aria-label`/`aria-current`
  to the nav and Wave 23 fixed brand contrast (WAVES.md log).
- **Status language is partly unified.** `.badge-success` / `.badge-pending` /
  `.badge-failed` (`BaseLayout.astro:150–152`) are the only badge variants and are
  used consistently where badges appear (36 usages across pages).

### Where consistency breaks (the evidence)

- **Four brand colours, one product.** System green `#0b7a55`
  (`BaseLayout.astro:33`) vs. landing-page **purple** `#6d28d9`
  (`pages/index.astro:235,259,360`) vs. login **blue** `#2563eb`
  (`pages/login.astro:140,157`) vs. register **teal** `#0f766e`
  (`pages/register.astro:98,131,155`). Android uses a fifth value, the *old*
  pre-accessibility green `#0E9F6E` (`android/.../values/themes.xml:4,7`) — the exact
  colour the web rejected at `BaseLayout.astro:31–32` for failing contrast (≈3.1:1).
- **The public pages re-implement the components the system already provides.**
  `pages/index.astro:226–243` defines its own `.btn` / `.btn-primary` /
  `.btn-secondary` (names that don't exist in BaseLayout, which has `.btn`,
  `.btn-ghost`, `.btn-danger`, `.btn-sm`). `pages/login.astro:122–165` and
  `pages/register.astro:111–176` style bare `input` / `button` / `label` elements
  with private borders (`#cbd5e1`), radii (0.5rem vs. system's 8px token), shadows
  (`0 10px 25px rgba(15,23,42,.08)` vs. `--shadow`) and focus rings — login's focus
  is blue, register's is teal, the system's is green (`BaseLayout.astro:128`).
- **There is no app shell.** `BaseLayout` renders a skip link and a `<slot />` and
  nothing else (`BaseLayout.astro:168–170`): no header, no wordmark, no footer, no
  account menu. The dashboard therefore has *no product identity and no log-out
  control anywhere* (grep for `logout`/`Log out` across `pages/` + `components/`
  returns nothing). The only navigation is `DashboardNav`, and it is rendered
  *outside* `<main>` and *outside* `.container` (e.g. `pages/dashboard/index.astro:10–12`),
  with no `max-width` or centring in its own styles (`DashboardNav.astro:88–98`) —
  so on wide screens the nav stretches edge-to-edge while the content below it sits
  in a centred 1120px column. The landing page invents the product's only header
  and footer (`pages/index.astro:88–97,168–179`), used nowhere else.
- **Stale version identity.** The shipped product is v0.0.2, but the landing page
  still announces "MiraclePay v0.0.1" twice in visible copy
  (`pages/index.astro:100,171`) and the admin header says "MiraclePay v0.0.1"
  (`pages/admin/index.astro:26`), despite Wave 139's version sweep and Wave 142's
  footer-version item in WAVES.md. File-header comments saying "v0.0.1" are
  historical and fine; *rendered* version strings are user-facing identity and are wrong.
- **A class that doesn't exist is used on the money pages.** Pay and checkout render
  their language toggle as `class="btn btn-small"` (`pages/pay/[slug].astro:24`,
  `pages/checkout/[id].astro:35`), and admin JS creates `btn btn-small` buttons
  (`pages/admin/index.astro:653`). BaseLayout defines `.btn-sm`, not `.btn-small`
  (`BaseLayout.astro:122`) — so these controls silently fall back to full-size
  `.btn` styling. Nobody noticed because nothing visually *breaks*; it just looks off.
- **i18n is two products deep.** `i18n.ts` holds ~66 keys, but `data-i18n` coverage is:
  pay 16, checkout 15, login 4, register 4, DashboardNav 1 (template), and **zero** on
  the landing page, both admin pages, and all eight dashboard pages' own content
  (the Wave 133 dictionary added nav + auth keys only). Worse, login and register
  *call* `applyI18n()` (`pages/login.astro:181–182`, `pages/register.astro:179–180`)
  but contain **no toggle control at all** — a Bangla-speaking user cannot switch
  language on the very pages where the translations exist. The toggle that does
  exist appears in three different forms: a pill inside the nav
  (`DashboardNav.astro:67`, with an inline `style=` hack), and a floated full-size
  button (`style="float:right"`) on pay/checkout. The header comment in
  `i18n.ts:4–7` still claims dashboard/admin "remain English in v0.0.1", contradicting
  Wave 133 — the docs drifted with the UI.
- **Merchant branding is wired but unsafe and incomplete.** Checkout and pay do apply
  `brand_color` (`pages/checkout/[id].astro:611–614`, `pages/pay/[slug].astro:448–450`),
  but they only set `--brand` and then *also* paint the submit button via inline
  `style.background` — because `--brand-dark` and `--brand-soft` are never recomputed,
  hover states and soft surfaces keep MiraclePay's green while the button turns the
  merchant's colour, and nothing checks the merchant colour's contrast against white
  button text. A merchant who picks pale yellow gets an unreadable Pay button on the
  most trust-sensitive page in the product.
- **No dark mode, anywhere on web.** `BaseLayout.astro:23` hard-codes
  `<meta name="color-scheme" content="light">` and there is no
  `prefers-color-scheme` block in the frontend. Android's theme is
  `Theme.MaterialComponents.DayNight` (`themes.xml:3`) — the phone app follows the
  OS, the website refuses to. The two platforms disagree about a basic user preference.

**One-sentence verdict for the team:** the dashboard is a design system; everything
around it is four hand-made brochures for the same company — Waves 151–200 exist to
make the brochures use the system.

---

## 2. Design principles MiraclePay should adopt

Concrete, testable, and written against the failures above. Each principle names
the check that proves it.

1. **One brand colour, everywhere, no exceptions.** MiraclePay green `#0b7a55`
   (with `--brand-dark` / `--brand-soft`) is the only primary colour on web and
   Android. Purple, blue and teal page palettes are deleted, not deprecated.
   *Check:* a CI grep for hex colours outside `BaseLayout.astro` (and the Android
   theme file) returns zero. Merchant brand colours are the single sanctioned
   exception, and only inside checkout/pay (see principle 6).
2. **Tokens before components, components before pages.** No page ships a `<style>`
   block that redefines a colour, radius, shadow, button, input, card or badge.
   If a page needs something new, it is added to the system (token or component
   class) first, then used. Spacing and type join colour as tokens: a 4-point
   spacing scale and a named type scale replace the current situation where
   `BaseLayout.astro:26–48` tokens cover colour/radius/font but every page still
   hard-codes its own `padding`/`font-size` literals.
   *Check:* page `<style>` blocks contain layout-only rules (grid placement,
   page-specific composition) and zero component redefinitions.
3. **One shell per audience.** Three shells, total: *Public* (landing + auth:
   shared header, wordmark, footer, language toggle), *App* (dashboard + admin:
   header with wordmark, merchant switcher, account/log-out, then nav, then the
   1120px container — nav inside the container, not floating above it), and
   *Payment* (pay + checkout: minimal, merchant-branded, no MiraclePay chrome
   competing with the merchant's). A user must never wonder which product they're in.
   *Check:* every page renders inside exactly one named shell component; no page
   hand-rolls a header or footer.
4. **The money pages are the design ceiling, not the exception.** Pay and checkout
   are the only pages most customers will ever see, and the only pages where a
   stranger must decide whether to trust the screen with money. They get the
   strictest rules: one primary action per state, amount always visible, status
   always worded (never colour alone), every state designed — loading, error,
   pending, submitted, processing, complete, failed/expired — in both languages.
   *Check:* a state-matrix screenshot set (7 states × 2 languages × 360px/1280px)
   exists and is reviewed before any checkout/pay change ships.
5. **Bangla is a first-class layout, not a translation layer.** Every user-facing
   string goes through `i18n.ts` (or doesn't ship), the language toggle is in the
   same place in every shell, and layouts are tested in Bangla at full string
   length — Bangla strings run long (see `pay_finish_body` in `i18n.ts`), so
   buttons, badges and nav pills must wrap or grow without breaking. The font
   stack gains a Bengali-capable face (e.g. "Noto Sans Bengali" in the `--font`
   stack) instead of relying on whatever the OS falls back to.
   *Check:* `data-i18n` (or `t()`) coverage is 100% of visible strings on public,
   auth and payment surfaces; dashboard/admin coverage is tracked to 100% by the
   end of the program, not "nav only".
6. **Merchant branding is a theme, not an override.** A merchant's brand colour
   generates a complete mini-theme (base, dark/hover, soft surface, and a
   contrast-checked on-colour) from one input, applied via tokens only — never
   inline `style.background` on individual elements. If the colour can't reach
   WCAG AA against white, the UI darkens it automatically and tells the merchant
   in Settings with a live preview.
   *Check:* set brand colour to `#ffe066` in a test: the Pay button remains
   readable (AA) and hover/soft states match the merchant hue, not MiraclePay green.
7. **Every state, every time: loading / empty / error / success look the same
   everywhere.** The patterns already exist in embryo — `.empty`, `.alert-error`,
   `role="status"` loading lines — but each page improvises (a muted `<p>` here,
   a `.card .empty` there, `role="alert"` divs vs. paragraphs). The system defines
   one Loading, one Empty (with icon slot, message, and optional action), one
   Error, one Toast/Status pattern, and pages compose them.
   *Check:* grep finds no page-local loading/empty/error markup that isn't the
   shared pattern.
8. **Design for the phone in the customer's hand and the phone on the merchant's
   counter.** Web is mobile-first (the only breakpoint today is 640px,
   `BaseLayout.astro:160`; the landing page invents a second one at 40rem,
   `pages/index.astro:395` — there will be one breakpoint scale). Android follows
   the same tokens (colour, radius, type sizes) in Material form, uses the same
   status vocabulary ("Forwarding on/off", "Queued", "Failed") as the web
   dashboard's device views, and respects DayNight because the web will too.
   *Check:* 360px-wide screenshot pass on every page; Android theme colours are
   generated from / diffed against the web token values.

---

## 3. Top 10 consistency problems, ranked

Ranked by (user impact × number of surfaces) ÷ effort. #1–#3 are identity-level:
a user can see them without opening dev tools.

### 1. Four competing brand palettes (landing purple, login blue, register teal, system green, Android old-green)
`pages/index.astro:235,259,360` (`#6d28d9`) · `pages/login.astro:140,157–165`
(`#2563eb`/`#1d4ed8`) · `pages/register.astro:98,131,155,163` (`#0f766e`/`#115e59`) ·
`BaseLayout.astro:33` (`#0b7a55`) · `android/.../themes.xml:4,7` (`#0E9F6E`).
The first three pages a user sees teach them three different brands; the dashboard
then teaches a fourth. This is the single most damaging inconsistency in the product.

### 2. Public/auth pages bypass the component system entirely
`pages/index.astro:226–243` redefines `.btn`/`.btn-primary`/`.btn-secondary`;
`pages/login.astro:87–176` and `pages/register.astro:81–176` restyle bare elements
with private hex borders (`#e2e8f0`, `#cbd5e1`), private shadows, private radii and
private focus colours, and don't use `.btn`, `.input`, `.label`, `.card` or
`.alert-*` at all — while dashboard pages use those classes 300+ times collectively.
Any future system change (radius, focus ring, dark mode) silently won't apply to
the pages new users land on.

### 3. No app shell: no header, wordmark, account menu or log-out in the dashboard; nav misaligned with content
`BaseLayout.astro:168–170` (slot only) · `DashboardNav.astro:88–98` (no max-width/
centring) vs. `BaseLayout.astro:91` (1120px centred container) · zero `logout`
matches in the frontend. Merchants get a floating pill-nav wider than their content
and no visible way to sign out of a payments product — a trust and safety problem,
not just an aesthetic one.

### 4. Bangla coverage is a façade outside pay/checkout, and the auth toggle doesn't exist
`i18n.ts` (~66 keys) vs. `data-i18n` counts: landing 0, admin 0/0, dashboard content
0 on all 8 pages, login/register 4 each — and login/register call `applyI18n()`
(`pages/login.astro:182`, `pages/register.astro:180`) with no toggle element on the
page. Three different toggle implementations where toggles do exist
(`DashboardNav.astro:67`; `pages/pay/[slug].astro:24`; `pages/checkout/[id].astro:35`).
For a Bangladesh-first product this is a product defect wearing a design costume.

### 5. Merchant brand colour breaks the theme it claims to apply
`pages/checkout/[id].astro:611–614` and `pages/pay/[slug].astro:448–450` set only
`--brand` plus an inline button background; `--brand-dark`/`--brand-soft` stay
MiraclePay green, and no contrast guard exists. Settings offers the colour picker
(`pages/dashboard/settings.astro:57–62`) with no preview and no warning. Merchants
*will* pick colours that make their own checkout look broken and unreadable.

### 6. Undefined `btn-small` class on pay, checkout and admin
`pages/pay/[slug].astro:24`, `pages/checkout/[id].astro:35`,
`pages/admin/index.astro:653` use `btn-small`; the system defines `.btn-sm`
(`BaseLayout.astro:122`). Symptom of the deeper problem: no component catalogue
anyone checks against, so class-name drift is invisible until someone greps.

### 7. Fifty inline `style=` attributes doing layout the system should own
`pages/dashboard/index.astro` ×22, `pages/dashboard/settings.astro` ×11,
`pages/dashboard/webhooks.astro` ×9, `pages/admin/index.astro` ×6 (plus
`style="float:right"` on both money pages). Mostly `margin: 0`, `justify-content`,
list resets — i.e. missing utility/composition classes (`.row-between`,
`.list-reset`). Inline styles can't respond to breakpoints, dark mode or the
language toggle, and they make the coming redesign a scavenger hunt.

### 8. Fourteen of fifteen pages carry private `<style>` blocks that duplicate each other
Every page except `pages/dashboard/settings.astro` has its own `<style>` block;
dashboard list pages (api-keys, invoices, payment-links, webhooks, plugins)
independently re-specify near-identical list/form/detail styles with slightly
different paddings and font sizes (e.g. `pages/dashboard/api-keys.astro:221–287`
vs. `pages/dashboard/invoices.astro:194–252`). Settings — the one page with *no*
style block — proves most pages need almost none. Duplication guarantees drift:
it already produced two breakpoints (640px vs. 40rem) and four palettes.

### 9. Stale/wrong user-facing version identity
Landing eyebrow + footer "MiraclePay v0.0.1" (`pages/index.astro:100,171`), admin
header "MiraclePay v0.0.1" (`pages/admin/index.astro:26`) — while the product is
v0.0.2 (WAVES.md Waves 149–150) and Wave 142 supposedly shipped a version footer.
Version display must come from one source (build-time constant), never hand-typed
copy. Users reporting bugs against "v0.0.1" that they're not running is an
operational cost, not a typo.

### 10. Platform disagreement: no web dark mode, dated Android theme, no shared tokens with Android
`BaseLayout.astro:23` forces `color-scheme: light`; Android uses DayNight
(`themes.xml:3`) but with the rejected `#0E9F6E` primary, default Material buttons,
8dp card radius vs. web's 12px, and hard-coded layout text (only 6 strings in
`strings.xml`; button labels live in `activity_main.xml`). Web and Android look
like cousins, not siblings — and a merchant who uses both daily notices.

*Honourable mentions (real, but below the cut):* status vocabulary beyond the three
badges (refunded/disputed/expired/cancelled) is mapped ad hoc in page JS; gateway
identities (bKash pink, Nagad orange…) have no defined colour/logo treatment and
render as plain text in a `<select>` on pay (`pages/pay/[slug].astro:100–107`);
checkout/pay use text glyphs (`!`, `✓`) as icons (`pages/checkout/[id].astro:45,181`)
and the product has no icon set at all; `.table` has no responsive card
transformation, only horizontal scroll (`BaseLayout.astro:145`).

---

## 4. What "done" looks like, per surface

Each definition is checkable: a reviewer with the built site, a 360px and a 1280px
viewport, and the Bangla toggle can verify every line. "Done" for Waves 151–200
means **all** bullets on that surface, not the average.

### Landing (`pages/index.astro`)
- Uses the Public shell: same header/footer component as auth; wordmark, nav and
  footer identical in markup and style to every other public page.
- Zero private component CSS: hero/feature/step/pricing composed from system
  classes; primary CTA is the system `.btn` in MiraclePay green; purple is gone
  (grep for `#6d28d9` returns nothing).
- Version string rendered from the single build-time version source; it reads
  v0.0.3 (the design release) at ship time, never a hand-typed literal.
- Fully translatable: every visible string has an `i18n` key; header contains the
  standard language toggle; Bangla at 360px wraps cleanly with no overflow.
- Gateway chips gain the defined gateway-identity treatment (name + brand mark/
  colour per the gateway style spec) instead of plain bordered text.
- Passes the shared checks: one breakpoint scale, dark-mode palette, focus order,
  200% zoom without horizontal scroll.

### Auth (`pages/login.astro`, `pages/register.astro`)
- Both pages are the *same* page pattern: one centred auth card from the system
  (`.card` + `.input` + `.label` + `.btn` + `.alert-error`), identical width,
  padding, shadow, heading scale and link styling; the only differences are the
  fields. Blue and teal palettes deleted.
- Language toggle present, in the Public-shell position, and functional — a user
  can switch to Bangla *before* creating an account.
- All states designed from system patterns: field error (inline, under the field,
  not only a top banner), form error, TOTP step (login's revealed 2FA field looks
  like a designed step, not an appendix), disabled/submitting, and the
  already-logged-in notice.
- Password requirements, hints and errors use `.field-hint` / alert patterns and
  are fully translated; no bare-element selectors remain in either file's styles.

### Dashboard (`pages/dashboard/*.astro`, `DashboardNav`)
- Rendered inside the App shell: header (wordmark, merchant switcher, account
  menu with log-out) + nav + content, all within the same centred measure; nav
  no longer spans wider than the content.
- `DashboardNav` becomes the shell's nav: same pill style, token-driven, with the
  language toggle in the header (not a pill among pills, no inline styles).
- All eight pages free of page-local component styles and inline `style=`
  attributes (grep returns zero for both); list/detail/form pages are built from
  shared List, Detail, Form and Stat patterns — the five list pages are visually
  indistinguishable in structure.
- Loading/empty/error/success use the single system patterns on every page,
  including the dashboard home's sub-panels (search, chart, KYC, 2FA), which today
  each improvise their own.
- Status badges cover the full transaction vocabulary (pending, completed, failed,
  refunded, disputed, expired, cancelled) with one mapping function shared by all
  pages; charts (dashboard home + admin analytics) use token colours only — no
  hard-coded `#0b7a55` in JS (`pages/dashboard/index.astro:620`) and no stray
  `--accent,#2563eb` fallback (`pages/admin/index.astro:601`).
- Bangla: nav, headings, table headers, empty states and actions translated;
  layout verified in Bangla at both viewports. Dark mode works and is the same
  palette as every other surface.

### Checkout / Pay (`pages/checkout/[id].astro`, `pages/pay/[slug].astro`)
- One Payment shell for both pages: same width (pay's 560px measure,
  `pages/pay/[slug].astro:148–150`, becomes the shared token), same brand line,
  same card, same amount typography, same footnote treatment — a customer moving
  from a payment link to a hosted checkout cannot tell where one product ended.
- Merchant theming is complete and safe: brand colour expands to base/dark/soft/
  on-colour via the theme generator; contrast-checked to AA; logo, brand message
  and merchant name follow one hierarchy on both pages; MiraclePay attribution is
  the small, consistent "Secure hosted checkout by MiraclePay" line on both.
- Gateway selection is a designed component (radio-style options with gateway
  identity marks and the wallet number/instructions per gateway), replacing the
  bare `<select>` on pay; checkout's instructions pattern and pay's pattern are
  the same component.
- The full state matrix is designed and reviewed in both languages: loading,
  error/expired, awaiting, submitted, processing, complete, failed — with real
  icons from the product icon set (not `!`/`✓` text glyphs), status shown in words
  + badge, and the reference/TrxID block copyable in one tap.
- The language toggle is the standard control in the standard position (no
  `float:right`, no `btn-small`); `btn-small` no longer exists anywhere.
- Trust details are consistent: amount + currency always visible above the fold,
  FX display-currency block identical on both pages, expiry/countdown styled the
  same, and nothing on either page uses a colour outside the merchant theme +
  system tokens.

### Admin (`pages/admin/index.astro`, `pages/admin/plugins.astro`)
- Visually a *mode* of the dashboard, not a separate product: same App shell,
  same patterns, with one deliberate differentiator (e.g. an "Admin" context
  badge/banner in the header) so an operator always knows they're acting
  platform-wide. The "Manage plugins" link becomes part of admin sub-navigation
  shared by both admin pages (today plugins is reachable only from a button on
  the admin home, `pages/admin/index.astro:29`).
- Dense data (users, merchants, KYC queue, audit log, analytics) uses the shared
  table pattern with consistent density, and destructive/consequential actions
  (suspend, KYC decisions, plugin enable/disable) use one confirm pattern and
  `.btn-danger` semantics everywhere.
- Version and platform strings come from the version source (the "v0.0.1" header
  text is gone); analytics bars/charts use token colours via the shared chart
  palette; no inline styles remain.
- Plugins page config forms (generated from `config_schema`) render with system
  form patterns, so a generated form is indistinguishable from a hand-built one.

### Android (`android/app/` — MainActivity, SettingsActivity)
- Theme rebuilt from the web tokens: primary is `#0B7A55` (the AA green — the
  `#0E9F6E` in `themes.xml:4,7` is retired on Android too), radius/type/spacing
  mapped to Material equivalents of the web scale (12dp card radius to match
  `--radius`, 16dp content padding standardised — today 20dp page / 16dp card,
  `activity_main.xml`), DayNight retained and matched to the web dark palette.
- All user-visible strings live in `strings.xml` (today: 6 strings; labels are
  hard-coded in layouts and Java Toasts) — the precondition for a Bangla Android
  UI, which is scoped and specced in this program even if it ships last.
- Status is a designed component, not three `TextView`s: forwarding state shown
  with the same vocabulary and colour semantics as the web (On/Off, Queued,
  Failed, Last success), the primary toggle is the visual anchor, and Settings /
  Test connection / permission / battery actions follow a clear primary-vs-
  secondary hierarchy instead of five identical full-width `Button`s
  (`activity_main.xml`).
- Empty/unconfigured, permission-denied, and queued/failed states each have a
  designed treatment with a next action, matching the web's Empty/Error patterns
  in content and tone.
- The app icon, wordmark and splash (if any) use the same MiraclePay identity as
  the web header — one logo, drawn once, used on all three surfaces.

---

## Appendix — audit method & limits

- Pages counted: 15 `.astro` files under `frontend/src/pages` (1 landing, 2 auth,
  8 dashboard, 2 admin, 1 checkout, 1 pay) — matches the v0.0.2 "15 pages" build
  claim in WAVES.md.
- Counts (class usage, `data-i18n`, inline styles, hex colours) are grep counts
  over `frontend/src` at audit time; they are reproducible with the patterns
  implied by the citations.
- This audit is source-based: it does not include rendered screenshots, device
  testing, or user research. Contrast figures quoted for the brand green are the
  ones recorded by the original authors at `BaseLayout.astro:31–32`. The UX
  Research and QA/Visual Regression seats should confirm the rendered reality
  (especially wide-screen nav alignment and merchant-colour behaviour) in their
  own audits.
- No source files were modified by this audit. The only file written is this one.
