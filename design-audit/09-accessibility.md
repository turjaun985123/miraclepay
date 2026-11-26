# 09 — Accessibility Audit (Wave 151)

**Role:** Accessibility Engineer, 10-agent design engineering team
**Scope audited (source only, no edits made):** `frontend/src/layouts/BaseLayout.astro`,
`frontend/src/components/DashboardNav.astro`, all 17 pages under `frontend/src/pages/**`,
`frontend/src/lib/i18n.ts`; Android `android/app/src/main/res/layout/activity_main.xml`,
`activity_settings.xml`, `res/values/themes.xml`, `res/values/strings.xml`, app
`build.gradle` (minSdk 23 / targetSdk 34 / compileSdk 34).
**Standard:** WCAG 2.2 AA. **Method:** static source audit + computed contrast
(WCAG relative-luminance formula, ratios rounded to 2 dp). No browser, screen reader,
TalkBack, or 200 %-zoom run was performed — items that need one are marked **[verify live]**.
**Prior round credited:** Waves 23/127 delivered the skip link, the darkened brand green,
`prefers-reduced-motion`, and (in the pages) `role="alert"`/`role="status"` regions,
`scope="col"` headers, `aria-current="page"`, and `document.documentElement.lang`
sync in `i18n.ts`. Those are real and are *not* re-listed as findings except where
they are incomplete.

**Headline verdict: NOT AA-conformant yet, but close on text contrast.** The shared
palette passes AA for almost every text pair actually rendered. The failures are
concentrated in (a) non-text contrast of control boundaries (1.4.11), (b) one
soft-tinted alert/badge pair, (c) merchant-supplied brand colour bypassing contrast
entirely on the two public money pages, (d) three forked per-page palettes that
re-introduce the exact pre-Wave-23 bug on Android, and (e) target size (2.5.8).

---

## 1. Contrast — pairs actually used (computed)

Palette (`BaseLayout.astro` `:root`): `--bg #f6f7fb`, `--surface #ffffff`,
`--ink #101828`, `--muted #667085`, `--brand #0b7a55`, `--brand-dark #095c40`,
`--brand-soft #e6f6ef`, `--border #e4e7ec`, `--danger #d92d20`,
`--danger-soft #fef3f2`, `--warning #b54708`, `--warning-soft #fffaeb`.
AA thresholds: 4.5:1 normal text, 3:1 large text (≥24 px / 18.66 px bold) and
3:1 non-text UI/graphical (1.4.11).

| # | Pair as used | Where | Ratio | AA |
|---|---|---|---|---|
| 1 | `#101828` on `#f6f7fb` | body text on page bg | **16.58** | ✅ |
| 2 | `#101828` on `#ffffff` | body text / `.btn-ghost` on cards | **17.75** | ✅ |
| 3 | `#667085` on `#ffffff` | `.muted`, `.field-hint`, table `th` (0.8125 rem, uppercase), `.stat-label`, placeholders | **4.97** | ✅ (margin 0.47) |
| 4 | `#667085` on `#f6f7fb` | `.muted`/`.empty` on bg, default `.badge` text | **4.65** | ✅ (margin 0.15 — fragile) |
| 5 | `#ffffff` on `#0b7a55` | `.btn` primary label (0.9375 rem, 600) | **5.34** | ✅ Wave 23 fix holds (was `#0e9f6e` = **3.39** ❌) |
| 6 | `#ffffff` on `#095c40` | `.btn:hover` | **8.03** | ✅ |
| 7 | `#095c40` on `#ffffff` / `#f6f7fb` | all links (`a { color: var(--brand-dark) }`) | **8.03 / 7.50** | ✅ |
| 8 | `#095c40` on `#e6f6ef` | `.badge-success`, `.alert-success`, nav `.is-active` | **7.18** | ✅ |
| 9 | `#ffffff` on `#d92d20` | `.btn-danger` (Revoke, Delete) | **4.83** | ✅ |
| 10 | `#ffffff` on `#b42318` | `.btn-danger:hover` | **6.57** | ✅ |
| 11 | `#d92d20` on `#ffffff` | danger text on surface | **4.83** | ✅ |
| 12 | `#d92d20` on `#fef3f2` | **`.badge-failed`, `.alert-error`** (0.8125–0.9375 rem) | **4.44** | ❌ **FAIL 1.4.3** by 0.06 |
| 13 | `#b54708` on `#fffaeb` | `.badge-pending` | **5.20** | ✅ |
| 14 | `#b54708` on `#ffffff` | warning text | **5.43** | ✅ |
| 15 | `#e4e7ec` on `#ffffff` / `#f6f7fb` | **input/select/textarea borders, `.btn-ghost` border, card & table borders, nav scrollbar** | **1.24 / 1.16** | ❌ **FAIL 1.4.11** (needs 3:1) — control boundaries are effectively invisible |
| 16 | `#e6f6ef` on `#ffffff` | `.input:focus` outline (`outline: 2px solid var(--brand-soft)`) | **1.12** | ❌ **FAIL 1.4.11 / 2.4.7 in effect** — see §2 |
| 17 | `#0b7a55` on `#ffffff` / `#f6f7fb` | global `:focus-visible` outline | **5.34 / 4.99** | ✅ |

### 1a. Forked palettes outside `BaseLayout` (the design-system leak)

Three surfaces ignore the tokens and hard-code their own colours. All were computed:

| Pair | Where | Ratio | AA |
|---|---|---|---|
| `#ffffff` on `#6d28d9`; `#6d28d9` on `#ffffff` | `pages/index.astro` landing CTA / links | 7.10 | ✅ |
| `#d1d5db`, `#e5e7eb` on `#ffffff`/`#f9fafb` | landing card/input borders | 1.47 / 1.18 | ❌ 1.4.11 |
| `#64748b` on `#ffffff` | `login.astro`, `register.astro` secondary text | 4.76 | ✅ (margin 0.26) |
| `#ffffff` on `#2563eb` / hover `#1d4ed8` | `login.astro` button (blue, not brand green) | 5.17 / 6.70 | ✅ |
| `#065f46` on `#ecfdf5`; `#b91c1c` on `#fef2f2` | login success / error banners | 7.29 / 5.91 | ✅ |
| `#cbd5e1` on `#ffffff` | login/register input borders | 1.48 | ❌ 1.4.11 |
| `#ffffff` on `#0f766e` / hover `#115e59` | `register.astro` button + link `#0f766e` | 5.47 / 7.58 | ✅ |
| `#475569` on `#ffffff` | register secondary text | 7.58 | ✅ |

So login/register/landing pass *text* contrast but fail the same border test, and a
user who learns "MiraclePay's button is green" meets a blue button on login and a
violet one on the landing page (consistency, 3.2.3/3.2.4 — advisory, not AA fail).

### 1b. Merchant brand colour — unbounded contrast hole (highest risk)

`checkout/[id].astro:611-614` and `pay/[slug].astro:448-450` accept any
`branding.brand_color` matching only `/^#[0-9a-fA-F]{6}$/`, set it as `--brand`, and
paint it directly as the submit button's background behind **white** text:

```js
document.documentElement.style.setProperty('--brand', brandColor);
if (submitButton) submitButton.style.background = brandColor;
```

`#ffffff` on e.g. `#ffd60a` = 1.41:1, on `#0e9f6e` = 3.39:1 — both fail, on the
"Pay now" button, for the least technical users in the product (payers, not
merchants). `dashboard/settings.astro:59-61,227-244` offers a free `<input type="color">`
+ hex field with **no contrast preview, warning, or clamp**. This is a live AA
failure whose value is not in the codebase. Fix in §5 (P0-2).

### 1c. Colour-alone / status

Good: statuses are never colour-only in the tables — `.badge-success/-pending/-failed`
always carry the status word as text, and checkout/pay status text is textual.
Watch: `.stat` deltas and the admin analytics "By gateway" bars **[verify live]**
must keep a text value next to any coloured bar.

---

## 2. Focus visibility (2.4.7, 2.4.13, 1.4.11)

- ✅ Global: `:focus-visible { outline: 2px solid var(--brand); outline-offset: 2px; }`
  — 2 px, offset, 5.34:1/4.99:1 against both backgrounds. Meets 2.4.7 and the
  2.4.13 *enhanced* (AAA) area/contrast tests for links and buttons.
- ❌ **Inputs override it into invisibility.** `.input:focus { border-color: var(--brand);
  outline: 2px solid var(--brand-soft); outline-offset: 0; }` replaces the strong
  outline with `#e6f6ef` at **1.12:1** on white. The only remaining cue is the border
  colour change `#e4e7ec → #0b7a55`, i.e. the focused field is identifiable only by
  a 1 px line whose *unfocused* state is itself sub-3:1. Every form in the product
  (login, register, all dashboard forms, checkout, pay) is affected.
- ⚠️ Login/register define their own `input:focus` (login: `2px solid #2563eb` =
  5.17:1 ✅; register: `2px solid #0f766e` = 5.47:1 ✅) — they are *better* than the
  shared component, which proves the shared rule is the outlier.
- ✅ Skip link: `.skip-link` off-screen at `top:-4rem`, revealed on `:focus` at
  `top:1rem`, points to `#main`; **every** page audited renders exactly one
  `<main id="main">`, so the target always exists. Not `display:none`/`visibility:hidden`,
  so it is keyboard-reachable. ✅ 2.4.1.
- ⚠️ Focus is never *moved* on async errors: pages show `#page-error[role=alert]`
  (announced, good) but several flows `scrollIntoView()` a success panel
  (`api-keys.astro:541`, `gateways.astro:508`) without moving focus to it, and
  `pay/[slug].astro:555` does handle `prefers-reduced-motion` for that scroll while
  the other two do not (their smooth scroll is still neutralised globally by the
  BaseLayout reduced-motion block, so this is cosmetic, not a fail).
- ⚠️ Focus order vs. visual order: `checkout/[id].astro:35` and `pay/[slug].astro:24`
  place the language toggle with `style="float:right"` as the **first** element in
  `<main>` — DOM order = first tab stop, visual position = top-right. Acceptable,
  but on a 320 px viewport it precedes the `<h1>` for AT users while appearing
  after it visually (1.3.2/2.4.3, minor). Both also use class `btn btn-small`;
  `BaseLayout` defines `.btn-sm`, **not** `.btn-small`, so the intended small size
  silently does not apply (they render at full `.btn` size — accidentally the more
  accessible outcome).

## 3. Keyboard operability (2.1.1, 2.1.3, 2.5.3, 4.1.2)

- ✅ No `onclick` divs/spans, no positive `tabindex`, no keyboard traps found in any
  page. All interactive elements in static markup are native `<a>`, `<button>`,
  `<input>`, `<select>`, `<textarea>`.
- ✅ JS-generated controls are also native: gateway picker cards are
  `document.createElement("button")` (`gateways.astro:389`), table row actions are
  real `<button>`s (Revoke, Copy, Send, Confirm/Cancel, item-remove). No
  `innerHTML`-injected pseudo-buttons were found.
- ✅ Language toggle in `DashboardNav.astro` is a real `<button type="button">` with
  `aria-label="Switch language / ভাষা বদলান"`; visible text (বাংলা) is contained in
  the accessible name (2.5.3 ✅).
- ✅ Destructive actions use native `window.confirm()` (`api-keys.astro:510`) —
  keyboard-operable and AT-announced, if ugly.
- ⚠️ **Repeated ambiguous action names (2.4.6 / 4.1.2, advisory→practical fail):**
  JS rows emit buttons whose entire accessible name is `Revoke`, `Copy`, `Send`,
  `Confirm`, `Cancel` with **no `aria-label` naming the row's key/link/transaction**
  (grep: zero `aria-label` in `transactions/invoices/webhooks/payment-links.astro`
  dynamic code). A screen-reader user in "buttons" navigation hears "Revoke,
  Revoke, Revoke". Fix: `aria-label="Revoke key {name}"` etc. (§5 P1-4).
- ⚠️ Native checkboxes/radios are 16×16 px (`gateways .check input { width:1rem;
  height:1rem }`, default elsewhere). The `<label>` row around them is taller and
  clickable, so pointer use is fine; the *box itself* is the focus/indicator target
  and is well under 24×24 (see §6).
- ⚠️ Horizontal-scroll regions (`DashboardNav` `overflow-x:auto`, `.table-wrap`)
  are reachable only by tabbing through their children; the nav container itself is
  not focusable, so a keyboard user on a narrow viewport cannot scroll it except via
  its links. Add `tabindex="0"` + `role="region"` + `aria-label` to `.table-wrap`
  instances, or guarantee no clipping at 320 px (§5 P2).
- ✅ No drag, path-based, or multi-pointer gestures anywhere (2.5.1/2.5.2/2.5.7 n/a);
  no time limits in the UI (2.2.1 n/a); no flashing content (2.3.1 ✅).
- ✅ Viewport: `width=device-width, initial-scale=1`, no `user-scalable=no` /
  `maximum-scale` — zoom to 200 %/400 % is not blocked (1.4.4/1.4.10), tables and
  nav degrade to scroll regions rather than clipping.

## 4. Labels, ARIA, forms, tables, headings

### Forms (3.3.1, 3.3.2, 3.3.3, 4.1.2, 1.3.5)

Per-page counts (static markup): inputs/labels/`for=` — admin 7/5/5, api-keys 2/2/1,
gateways 5/6/4, dashboard 11/15/15, invoices 5/6/6, payment-links 5/7/5,
settings 9/11/11, transactions 1/2/2, webhooks 3/4/4, login 3/3/3, register 3/3/3,
checkout 2/3/3, pay 3/5/5. **No unlabelled static input was found**; the deltas are
checkboxes wrapped in their `<label>` (api-keys scope options `api-keys.astro:121`,
gateways `.check` rows) and admin's `aria-label`-only audit filters
(`admin/index.astro:250-251`), both valid naming techniques.

- ✅ `autocomplete` is used correctly and widely (1.3.5): `email`, `current-password`,
  `new-password`, `one-time-code` (login TOTP, dashboard 2FA), `name`, `organization`,
  `tel`, `url`. Register's password hint is wired with
  `aria-describedby="password-hint"` (`register.astro:55`) — the only
  `aria-describedby` in the codebase.
- ❌ **Hints are not programmatically associated elsewhere.** `.field-hint` text
  (formats, "empty = all", sandbox warnings) is visual-only on every other form;
  `aria-invalid` appears **nowhere**, and errors are reported only in a page-level
  `#page-error[role=alert]` paragraph, not on the offending field. 3.3.1 is met at
  page level (error is announced as text) but a user must hunt for *which* field;
  3.3.3 error-suggestion is weak (messages like "Could not load…" give no field fix).
  Per-field validation that does exist (payment-links/checkout/pay) does call
  `.focus()` on the bad input (`payment-links.astro:526,548,561`,
  `checkout/[id].astro:726,731`, `transactions.astro:375`) — good pattern, extend it.
- ⚠️ Placeholder-as-supplement: placeholders reuse `--muted` (4.97:1 ✅) and never
  replace a label in static markup. Admin audit filters rely on placeholder +
  `aria-label` only — acceptable, but the visible label is absent (3.3.2 advisory).
- ⚠️ `<input type="color" id="brand-color">` in settings is paired with the hex text
  field and synced both ways — good; it needs its own visible label check **[verify live]**
  and is the entry point for the §1b contrast hole.
- ✅ Required-state: forms lean on native `required`/type validation; no
  colour-only required markers were found.

### Tables (1.3.1)

- ✅ Every static table uses `<th scope="col">` (admin 23, api-keys 7, webhooks 12,
  transactions 8, invoices 6, payment-links 5, dashboard 5). JS-built rows use `<td>`
  with text badges, so header association survives.
- ⚠️ **Zero `<caption>` elements in the whole frontend** and tables are not wrapped
  in `aria-labelledby` regions in most pages — a table's purpose is only inferable
  from the preceding `<h2>`. Not an AA failure (headings precede them), but add
  `<caption class="visually-hidden">` for list-navigation users.
- ⚠️ `.visually-hidden` is defined **locally in `api-keys.astro:197` only**, yet the
  pattern (e.g. the "Actions" `<th>` at `api-keys.astro:157`) is the only place it is
  used. Any other page copying that markup without the local style would expose or
  break it. Promote it to `BaseLayout`.
- ✅ Empty states are text (`.empty`), loading states use `role="status"`, errors
  `role="alert"` — counts of `role=` per page: dashboard 12, admin 9, api-keys 5,
  webhooks 5, checkout 5, gateways 4, payment-links 4, invoices 3, settings 3,
  pay 2, transactions 1. Transactions is the thinnest: its async status changes are
  the least announced **[verify live]**.

### Headings & landmarks (1.3.1, 2.4.6, 2.4.2)

- ✅ One `<main id="main">` per page, one `<nav aria-label="Dashboard">`, page
  `<title>` = `MiraclePay — {title}`. Landmark structure is clean.
- ⚠️ **Multiple `<h1>`s in the DOM on the two public pages.** `checkout/[id].astro`
  has `<h1>Checkout unavailable</h1>` (in a `hidden` section) **and**
  `<h1 id="merchant-name">Merchant</h1>`; `pay/[slug].astro` has **three**:
  error-card `<h1>`, `<h1 id="pay-title">Payment</h1>`, and success-card
  `<h1>Payment started</h1>`. `hidden` removes the inactive ones from the a11y tree,
  so at any instant only one is exposed — not a hard fail — but it is fragile: any
  flow that un-hides two states (error + form) produces two `<h1>`s, and heading
  navigation during load can land on the wrong one. Demote the error/success
  headings to `<h2>` and keep a single persistent `<h1>` (§5 P1-5).
- ✅ Otherwise heading order is strictly hierarchical with no skipped levels:
  dashboard `h1→h2→h3` (Get started → Transactions/Payment links/Invoices),
  admin `h1→h2→h3` (Analytics → By gateway), landing `h1→h2→h3`. No heading is
  used purely for styling in the pages checked.
- ⚠️ `html lang="en"` is static in `BaseLayout`, but `i18n.ts` keeps
  `document.documentElement.lang` in sync when the user switches to Bangla (bn) —
  ✅ 3.1.1/3.1.2 **provided** `applyI18n()` runs on every page that renders Bangla.
  DashboardNav calls it; checkout/pay have their own toggles. Pages that render
  `data-i18n` strings without calling it would announce Bangla text as English
  **[verify live]**.

### Motion & misc

- ✅ `prefers-reduced-motion: reduce` block (Wave 23) kills transitions/animations
  and smooth scroll globally (2.3.3 advisory / 2.2.2). No autoplay, carousels, or
  parallax anywhere.
- ✅ Text spacing/reflow: relative units throughout, `line-height 1.55`, no fixed
  heights on text containers; 320 px media query reduces padding/type. 1.4.10/1.4.12
  expected pass **[verify live at 400 %]**.
- ⚠️ `color-scheme: light` only and no dark-mode palette — not an AA criterion, but
  Android *does* ship DayNight (see §7), so the two platforms diverge.

---

## 5. Ranked fix list (concrete values)

### P0 — AA failures on money paths

1. **Input borders / control boundaries (1.4.11, all pages).** `--border #e4e7ec`
   (1.24:1) is used for the *only* boundary of inputs, selects, textareas and
   `.btn-ghost`. Introduce a separate token so decorative card borders can stay light:
   `--border-strong: #8a94a6` (**3.06:1** on `#ffffff`, 2.86:1 on `#f6f7fb`) as the
   minimum, or preferably `--border-strong: #7d879c` (**3.61:1** white / 3.37:1 bg)
   for inputs/selects/textarea/`.btn-ghost`. Apply the same to the forked palettes:
   login/register `#cbd5e1` (1.48) → `#7d879c`; landing `#d1d5db` (1.47) → `#7d879c`.
   Keep `--border #e4e7ec` only for card/table *row* separators that are not the
   sole means of identifying a control.
2. **Merchant brand colour gate (1.4.3, checkout + pay).** In `settings.astro`,
   `checkout/[id].astro`, `pay/[slug].astro`: compute the ratio of the chosen colour
   against `#ffffff` before applying it as a button background; if `< 4.5:1`,
   (a) in settings, block/warn with the computed ratio and suggest the darkened value,
   (b) at render time, fall back to a darkened variant or switch the button label to
   `#101828` when that passes (it does for light brand colours: `#101828` on
   `#ffd60a` ≈ 11.9:1). Never paint white text on an unvalidated colour. Also apply
   the validated colour to `--brand-dark` for link text, which currently inherits
   the raw `--brand` in places (`checkout` left-border, pay `pay-brand-message`).
3. **Android primary colour (see §7).** `themes.xml` `colorPrimary #0E9F6E` with
   `colorOnPrimary #FFFFFF` = **3.39:1** — the exact pre-Wave-23 web bug, still live
   on Android. Change `colorPrimary` → `#0B7A55` (**5.34:1**, already
   `colorPrimaryVariant`) and `colorSecondary` → `#0B7A55`.
4. **Failed/error soft pair (1.4.3).** `.badge-failed` + `.alert-error`:
   `#d92d20` on `#fef3f2` = 4.44:1. Change the *text* colour in soft contexts to
   `#b42318` → **6.05:1** on `#fef3f2` (this hex is already in the codebase as
   `.btn-danger:hover`). Keep `#d92d20` for white-background text and button fills
   (4.83:1 ✅). Equivalent alternative: darken `--danger-soft` text via a new
   `--danger-dark: #b42318` token.

### P1 — serious, cheap

5. **Input focus indicator (2.4.7/1.4.11).** Replace
   `.input:focus { outline: 2px solid var(--brand-soft); outline-offset: 0; }`
   with `outline: 2px solid var(--brand); outline-offset: 0; border-color: var(--brand-dark);`
   → 5.34:1 outline, matching the global `:focus-visible` treatment login/register
   already have.
6. **Target size (2.5.8 AA — new in WCAG 2.2; 2.5.5 AAA).** See §6: `.btn-sm`
   (~34–36 px tall) → add `min-height: 24px` is *not* enough for usability; set
   `.btn-sm { min-height: 40px; padding: 0.5rem 0.75rem; }` and
   `.dashboard-nav-link { min-height: 40px; }` (currently ~35 px). Both already pass
   the 2.5.8 *minimum* of 24×24 CSS px, so this is an AAA/usability fix, not an AA
   blocker — except inline text links and the 16 px checkbox boxes, which rely on
   the 2.5.8 spacing exception and should be confirmed not to be within 24 px of
   another target **[verify live]**.
7. **Name the row actions.** Add `aria-label`s to JS-created buttons:
   `Revoke key "{name}"`, `Copy link "{title}"`, `Send invoice {number}`,
   `Confirm transaction {reference}` / `Cancel transaction {reference}`.
8. **One `<h1>` per page.** Demote checkout error `<h1>` and pay error/success
   `<h1>`s to `<h2>`; keep `merchant-name` / `pay-title` as the sole `<h1>`.
9. **Per-field errors.** Add `aria-describedby` from each input to its hint **and**
   error node, set `aria-invalid="true"` on failure, and keep the existing
   `.focus()`-the-field pattern. Start with login, register, checkout, pay (money +
   credential paths), then dashboard create-forms.

### P2 — polish / robustness

10. Promote `.visually-hidden` into `BaseLayout`; add `<caption class="visually-hidden">`
    to all 13 static tables; give `.table-wrap` `tabindex="0" role="region"
    aria-label="{table name}"` so keyboard users can scroll wide tables.
11. Collapse the three forked palettes (landing violet `#6d28d9`, login blue
    `#2563eb`, register teal `#0f766e`) onto the BaseLayout tokens; delete the
    per-page hexes. This is the root cause of findings 1, 5 and §1a.
12. Raise `.muted` safety margin: `#667085` passes on bg by only 0.15 (4.65:1) and
    **fails (4.45:1) if ever placed on `--brand-soft`**. Either forbid muted-on-soft
    or move muted text to `#475467` (**7.69:1** white / 7.18:1 bg / also passes on
    `#e6f6ef`). Recommended: `#475467` for `.stat-hint`, `.field-hint`, table `th`.
13. `lang` assurance: assert `applyI18n()` runs on checkout/pay before first paint
    of Bangla strings; add `lang="bn"` spans for the nav toggle's Bangla label.
14. Add an automated gate so this does not regress again: axe-core + a contrast
    assertion over the token table in CI for the frontend, and Android Lint
    (`contentDescription`, `TouchTargetSizeCheck`, `HardcodedText`) in the Android
    CI job (see §7 for why lint currently has nothing to catch).

---

## 6. Touch / pointer target sizes — web (2.5.8 AA ≥24×24, 2.5.5 AAA ≥44×44)

Computed from `BaseLayout` padding + font-size at the 16 px root:

| Control | CSS | Approx. height | 2.5.8 (24) | 2.5.5 (44) |
|---|---|---|---|---|
| `.btn` | `padding .5625rem 1rem`, 0.9375 rem text | ~41–43 px | ✅ | ⚠️ just under |
| `.btn-sm` (Revoke, Copy, Send, Confirm, Refresh, Add item) | `padding .375rem .75rem`, 0.875 rem | ~34–36 px | ✅ | ❌ |
| `.btn` on checkout/pay (`.btn-small` — undefined class) | falls back to `.btn` | ~41–43 px | ✅ | ⚠️ |
| `.dashboard-nav-link` (incl. language toggle) | `padding .375rem .75rem`, 0.875 rem | ~35 px | ✅ | ❌ |
| `.input` / `select.input` | `padding .5625rem .75rem` | ~41–43 px | ✅ | ⚠️ |
| `.skip-link` (when focused) | `padding .5rem 1rem` | ~39 px | ✅ | ⚠️ |
| checkbox/radio box | 16×16 px (`.check input`), browser default elsewhere | 16 px | ❌ box alone; ✅ via spacing exception *if* the label row keeps ≥24 px clear of other targets | ❌ |
| Body text links (`a`) | inline | line-height ~25 px | ✅ (inline exception) | n/a |

No target measured below 24 px except the checkbox box itself; nothing reaches
44 px. **Verdict: 2.5.8 AA pass (with the checkbox spacing caveat), 2.5.5 AAA fail
across the board.** Recommended floor for the design system: 40 px height for all
button/nav variants, 44 px on the two public payer pages.

---

## 7. Android (`activity_main.xml`, `activity_settings.xml`, `themes.xml`)

### Content descriptions

- ✅ **No `ImageView`, `ImageButton`, or icon-only control exists in either layout** —
  every control is a text `Button`, a labelled `SwitchCompat`, or an `EditText`, so
  there is nothing that *needs* a `contentDescription`, and correctly none is
  invented. `grep contentDescription|setContentDescription` over the whole app:
  **0 hits**, and that is a pass here, not a gap. The only icon in the app is the
  notification small icon (`Notify.java:57`, `android.R.drawable.stat_notify_sync`),
  which is decorative to the notification title/text and needs no description.
- ⚠️ The app title "MiraclePay" + subtitle are plain `TextView`s, not marked as
  headings: add `android:accessibilityHeading="true"` (API 28+, minSdk is 23 so
  guard or accept graceful no-op via `androidx.core`) to the 24 sp title in
  `activity_main.xml` so TalkBack heading navigation works.
- ⚠️ Dynamic status is not announced: `statusText`, `configSummaryText`, `statsText`
  in `activity_main.xml` are updated from `MainActivity` with no
  `android:accessibilityLiveRegion="polite"` (or `announceForAccessibility`).
  A TalkBack user who taps "Test connection" hears nothing when the result lands.
  Set `accessibilityLiveRegion="polite"` on `statusText` (and `deviceIdText` is
  already `textIsSelectable="true"` ✅).
- ✅ Settings labels are done properly and unusually well: each `EditText` has a
  separate `TextView` with `android:labelFor="@id/…"` for `serverUrlEdit`,
  `deviceKeyEdit`, `merchantLabelEdit`, `allowedSendersEdit`, plus correct
  `inputType` (`textUri`, `textPassword`, `textMultiLine`), `autofillHints`
  (`url`, `password`) and `importantForAutofill`. Hint text supplements, never
  replaces, the label. ✅
- ⚠️ `activity_main.xml`'s `SwitchCompat#enableSwitch` carries its label as its own
  `android:text="Forward payment SMS"` ✅, but its state is conveyed by the switch
  only — `statusText` should mirror on/off in text (it partially does via status).
- ⚠️ Hard-coded strings: 8 `android:text="…"` literals in `activity_main.xml` and 7
  in `activity_settings.xml`, while `strings.xml` holds only 6 strings and is barely
  used. Not itself a WCAG failure, but it blocks Bangla localisation of the Android
  UI (the web side has bn; Android is English-only) — 3.1.2/consistency gap for the
  product's primary market.

### Touch targets

- ✅ Material `Button`s (Settings, Test connection, Grant SMS permission, Retry,
  Allow background running, Save) use the platform default `minHeight` 48 dp and
  are full-width (`match_parent`) → **≥48×48 dp, exceeds WCAG 2.5.5's 44 and
  Android's 48 dp guideline**. Vertical spacing is 8–16 dp between them.
- ⚠️ `SwitchCompat#enableSwitch`: `layout_height="wrap_content"` with only
  `paddingTop/Bottom 8dp` around 16 sp text → ≈40–44 dp tall touch row, and the
  switch thumb track itself is the standard 48×~28 dp widget. Set
  `android:minHeight="48dp"` on the switch row to be safe.
- ✅ `EditText`s are full-width, single-line fields at platform default height
  (~48 dp); the multi-line senders field has `minLines="3"`. No target under 48 dp
  wide exists in either layout.
- ✅ No gesture-only actions; everything is a tap on a labelled control.

### Text scaling & layout

- ✅ **All text sizes are in `sp`** (24/16/14 sp in main, theme text appearances in
  settings) — no `dp`/`px` text anywhere, so system font-scale (up to 200 %) applies.
- ✅ Both screens are `ScrollView` + vertical `LinearLayout` with `wrap_content`
  heights and no fixed-height text containers, so enlarged text scrolls instead of
  clipping; `fillViewport="true"` is set. No `android:lines`/`maxLines`/`ellipsize`
  truncation was found.
- ⚠️ Risks at 200 % font scale / small screens **[verify on device]**: the stats
  line ("Forwarded: 0 · Failed: 0 · Filtered: 0 · Last success: —") is one long
  `TextView` that will wrap to 3–4 lines inside the card — it wraps (good) but the
  card has no heading separating it from `configSummaryText`; and 20 dp screen
  padding + 16 dp card padding leaves a narrow measure at 320 dp width.
- ✅ Dark mode: `Theme.MaterialComponents.DayNight.NoActionBar`, secondary text uses
  `?android:attr/textColorSecondary` (theme-resolved, AA in both modes by platform),
  no hard-coded text colours in layouts.
- ❌ **Theme colour failure (P0-3 above):** `colorPrimary #0E9F6E` /
  `colorOnPrimary #FFFFFF` = **3.39:1** for button labels and the app bar accents;
  `colorPrimaryVariant #0B7A55` (5.34:1) is already defined and is used only for the
  status bar. Swap them.
- ⚠️ No `android:layoutDirection`/RTL or `supportsRtl` check was in scope of the
  layouts read; Bangla is LTR so this is not a current-market blocker.

---

## 8. Summary scorecard

| Area | Result |
|---|---|
| 1.4.3 Text contrast (shared palette) | **Pass except** `.alert-error`/`.badge-failed` on `--danger-soft` (4.44) and unvalidated merchant brand colour |
| 1.4.11 Non-text contrast | **Fail** — input/ghost borders 1.24:1, forked palettes 1.47–1.48:1 |
| 1.4.1 / 1.3.3 Colour-alone, status | Pass (text always accompanies colour) |
| 2.4.7 / 2.4.13 Focus visible | **Fail for inputs** (1.12:1 outline); pass for links/buttons |
| 2.4.1 Bypass blocks | Pass (skip link + single `<main id="main">` on all 17 pages) |
| 2.1 Keyboard | Pass (all native controls, incl. JS-created); minor: scroll regions not focusable |
| 3.3 / 4.1.2 Labels & errors | Mostly pass; no `aria-invalid`, hints unassociated, ambiguous JS button names |
| 1.3.1 Headings/tables | Pass with warnings: 2–3 `<h1>`s in checkout/pay DOM, 0 `<caption>`s |
| 2.5.8 Target size (AA, 24 px) | Pass (checkbox box excepted via spacing) |
| 2.5.5 Target size (AAA, 44 px) | Fail — web max ≈43 px; Android passes at 48 dp |
| 2.3 / 2.2 Motion & timing | Pass (reduced-motion block, no autoplay/timers) |
| Android descriptions / scaling | Pass (no icon-only controls; `labelFor` exemplary; all `sp`, scrollable) except live-region + heading semantics and the 3.39:1 primary colour |

*Audit only — no source files were modified.*
