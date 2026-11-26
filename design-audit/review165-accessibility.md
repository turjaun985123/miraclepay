# Review 165 — Accessibility Re-Review (Wave 165, Design Review #1)

**Role:** Accessibility Engineer. **Scope re-checked (source only, no edits):**
`frontend/src/styles/tokens.css`, `frontend/src/layouts/BaseLayout.astro`,
`frontend/src/pages/index.astro` (new landing), `login/register/forgot-password.astro`
(auth pages), `frontend/src/components/DashboardNav.astro`, `frontend/src/lib/ui.ts`
(toast/dialog/field-error helpers), plus spot re-greps of `checkout/[id].astro`,
`pay/[slug].astro`, `dashboard/settings.astro`, `dashboard/api-keys.astro`, and
Android `res/values/themes.xml` — each against its Wave 151 item in
`design-audit/09-accessibility.md` (§5 fix list).
**Standard:** WCAG 2.2 AA. **Method:** static source review + computed contrast
(WCAG relative-luminance formula, ratios rounded to 2 dp), light and dark.
No browser, screen-reader, TalkBack, or zoom run — **[verify live]** items carried.

**Headline: 4 PASS, 3 PARTIAL, 4 FAIL of the 11 tracked items.** The token system
fixed the three shared-component failures it aimed at (danger-soft text,
border-strong, input focus) and the forked palettes are gone — but the two
highest-risk P0s (merchant brand colour, Android primary) are byte-for-byte
untouched, and the new dark mode ships **two new AA failures** of its own
(§3). Dark-mode *text* pairs otherwise pass comfortably.

---

## 1. Wave 151 P0 items — verdicts

### P0-1 · Input/control borders, `--border-strong` (1.4.11) — ✅ PASS

`tokens.css` adds exactly the prescribed token: `--color-border-strong: #7d879c`.

| Pair | Ratio | AA (≥3:1 non-text) |
|---|---|---|
| `#7d879c` on `#ffffff` (surface) | **3.61** | ✅ |
| `#7d879c` on `#f6f7fb` (bg) | **3.37** | ✅ |
| Dark: `#98a2b3` on `#101828` (surface) | **6.89** | ✅ |
| Dark: `#98a2b3` on `#0b1220` (bg) | **7.27** | ✅ |

Applied where 09 asked: `.input`/`select`/`textarea` borders, `.btn-ghost`,
`.copy-field` dashed border, landing `.chip`. The forked-palette instances are
gone with the palettes themselves (P2-11, §4). Caveat, not a fail: decorative
`--color-border` stays `#e4e7ec` = 1.24:1 light, and dark `#344054` on `#101828`
= **1.70:1** — acceptable only while it is never a control's sole boundary;
`.badge`'s border is decorative (text + soft fill carry it). **[verify live]**
that no page promotes `--color-border` to an input boundary locally.

### P0-2 · Merchant brand-colour gate, checkout + pay (1.4.3) — ❌ FAIL, unchanged

Identical code to Wave 151: `checkout/[id].astro:611-614` and
`pay/[slug].astro:448-450` still accept any `/^#[0-9a-fA-F]{6}$/` colour, set it
as `--brand`, and paint it as the submit button's background behind white text
(white on `#ffd60a` = 1.41:1). `dashboard/settings.astro:227-244` still syncs
the colour/hex fields with **no ratio computation, warning, clamp, or preview**.
This remains the single highest-risk item in the product: an unbounded,
merchant-controlled AA failure on the "Pay now" button for payers. Note it now
also interacts with dark mode — the raw colour overwrites `--brand` in both
schemes with no validation in either.

### P0-3 · Android `colorPrimary` (1.4.3) — ❌ FAIL, unchanged

`android/app/src/main/res/values/themes.xml:4,7` still:
`colorPrimary #0E9F6E`, `colorSecondary #0E9F6E`, white on it = **3.39:1**.
`colorPrimaryVariant #0B7A55` (5.34:1) is still defined and still only used for
the status bar. One-line fix, still not taken. (Carried Android items also
unchanged: no `accessibilityLiveRegion` or `accessibilityHeading` in either
layout — grep 0 hits.)

### P0-4 · Failed/error soft pair, `--danger-soft` (1.4.3) — ✅ PASS (light and dark)

Fixed exactly as prescribed, via a dedicated token
`--color-danger-text: #b42318`, now used by `.badge-failed`, `.alert-error`,
and `.field-error`:

| Pair | Ratio | AA (≥4.5:1) |
|---|---|---|
| Light: `#b42318` on `#fef3f2` (danger-soft) | **6.05** | ✅ (was 4.44 ❌) |
| Light: `#b42318` on `#ffffff` (field-error) | **6.57** | ✅ |
| Dark: `#f97066` on `#3a1512` (danger-soft) | **5.82** | ✅ |
| Dark: `#f97066` on `#101828` (danger text on surface) | **6.37** | ✅ |

`#d92d20` is retained only for white-background text and button fills
(white on it = 4.83 ✅ light). The dark button fill is a different story — §3.

---

## 2. Wave 151 P1 items — verdicts

### P1-5 · Input focus indicator (2.4.7 / 1.4.11) — ✅ PASS

`.input:focus` is now `border-color: var(--color-brand); outline: 2px solid
var(--color-focus); outline-offset: 1px;` — the invisible `#e6f6ef` outline
(1.12:1) is gone.

| Focus outline pair | Ratio |
|---|---|
| Light `#0b7a55` on `#ffffff` / `#f6f7fb` | **5.34 / 4.99** ✅ |
| Dark `#34c98e` on `#101828` / `#0b1220` | **8.36 / 8.82** ✅ |

Global `:focus-visible` (2 px, offset 2 px, same colours) unchanged and passing.

### P1-6 · 44 px targets (2.5.8 AA / 2.5.5 AAA) — ⚠️ PARTIAL

Real progress, one deliberate exception that swallows much of it:

| Control | Height now | 44 px floor |
|---|---|---|
| `.btn`, `.input`/`select`/`textarea` (`min-height: var(--target-min)` = 2.75 rem) | 44 px | ✅ |
| `.dashboard-nav-link` (`min-height: var(--target-min)`) | 44 px | ✅ (was ~35 px) |
| Landing `.chip`, `.btn` CTAs | 44 px | ✅ |
| **`.btn-sm` / `.btn-small`** (`min-height: 2.25rem`) | **36 px** | ❌ |

`--target-min: 2.75rem` exists and BaseLayout's comment calls `.btn-sm` "the
dense-table exception" — but it is not confined to dense tables: it styles the
DashboardNav language toggle and Log out buttons, and `.btn-small` is the
checkout/pay language toggle. Side effect worth naming: at Wave 151 the
undefined `.btn-small` class accidentally rendered those payer-page toggles at
full `.btn` size (~41–43 px); the new alias makes them an intentional 36 px —
a small target-size *regression* on the two public money pages, traded for
visual consistency. Everything still passes 2.5.8 AA (≥24 px); 2.5.5 AAA and
the system's own stated 44 px floor fail wherever `-sm` is used. Checkbox
boxes remain 16×16 px (`gateways`, `payment-links` `.check input`) — unchanged,
still relying on the 2.5.8 spacing exception. Landing/footer text links have
no `min-height` (~25 px line box, well-spaced — spacing exception, advisory).

### P1-7 · Named row actions (2.4.6 / 4.1.2) — ❌ FAIL

JS row buttons are still bare: `api-keys.astro:478`
`revokeButton.textContent = 'Revoke'` with no `aria-label`; grep finds **zero**
`aria-label` set on any JS-created row action across
transactions/invoices/webhooks/payment-links/api-keys. (Static sections did
gain thorough `aria-labelledby` throughout the dashboard — good, but that was
not this item.) Screen-reader button navigation still hears "Revoke, Revoke,
Revoke".

### P1-8 · One `<h1>` per page (1.3.1 / 2.4.6) — ❌ FAIL (improved on checkout only)

- `checkout/[id].astro`: success/status headings were demoted to `<h2>` ✅,
  but the DOM still holds **two** `<h1>`s — error card (line 46) and
  `#merchant-name` (line 66).
- `pay/[slug].astro`: still **three** `<h1>`s — error (30), `#pay-title` (41),
  and "Payment started" success (114).

`hidden` still keeps only one in the a11y tree at a time, so this remains
fragile-not-hard-fail — but the prescribed demotion was only half done, and
not on pay at all.

### P1-9 · Per-field errors (3.3.1 / 3.3.3 / 4.1.2) — ⚠️ PARTIAL (helper built, never used)

`lib/ui.ts` now ships exactly the prescribed machinery — `fieldError()` sets
`aria-invalid="true"`, inserts a `.field-error` node, and wires
`aria-describedby`; `clearFieldError()` reverses it; BaseLayout styles both
(`.input[aria-invalid="true"]`, `.field-error` in `--color-danger-text`,
6.57:1 on white ✅). **Adoption is zero:** grep finds no `fieldError` /
`clearFieldError` call site in any page. Login still reports only via the
page-level `#login-error[role=alert]` (announced ✅, and it does `.focus()` the
empty field ✅) with no `aria-invalid` and no field-level message. Register and
forgot-password keep their Wave 151 `aria-describedby` hint wiring. The
infrastructure passing while the money/credential forms don't use it is the
Wave 151 "built but unwired" pattern recurring.

---

## 3. Dark mode — computed pairs (new since Wave 151)

Dark mode is new (`prefers-color-scheme` + `data-theme="dark"`, semantic layer
only; `color-scheme: light dark` now declared). Requested pairs first:

| Pair (dark) | Ratio | Verdict |
|---|---|---|
| Brand `#34c98e` on `#101828` (surface) | **8.36** | ✅ |
| Brand `#34c98e` on `#0b1220` (bg) | **8.82** | ✅ |
| Danger `#f97066` on `#3a1512` (danger-soft) | **5.82** | ✅ |
| Muted `#98a2b3` on `#0b1220` (bg) | **7.27** | ✅ |
| Muted `#98a2b3` on `#101828` (surface) | **6.89** | ✅ |

Supporting dark pairs, all ✅: ink `#f2f4f7` on bg/surface **16.99 / 16.11**;
ink-2 `#d0d5dd` on bg **12.70**; links `#5ed6a5` on bg/surface **10.38 / 9.83**;
on-brand `#06231a` on brand `#34c98e` **7.83**, on brand-hover `#5ed6a5`
**9.21**; badge-success `#5ed6a5` on `#0c2b21` **8.41**; warning `#fdb022` on
`#342605` **8.00**; info `#84adff` on `#10264d` **6.69**; accent text `#a78bfa`
on bg **6.88**; border-strong **6.89 / 7.27** (§1). Notably, dark muted (7.27)
has a *larger* safety margin than light muted (4.65, and 4.45 on brand-soft —
P2-12 remains open in light only).

**Two new dark AA failures, both from hard-coded white foregrounds:**

| Pair | Ratio | Where |
|---|---|---|
| `#ffffff` on `#f97066` | **2.79** ❌ | `.btn-danger` — BaseLayout sets `color: var(--p-white)` unconditionally, but dark `--color-danger` is the light red-400. Every destructive button (Revoke, Delete, dialog confirm) fails 1.4.3 in dark mode. Hover is also ❌: white on `#f04438` = **3.76**. Fix: in dark, give `.btn-danger` `color: var(--p-gray-950)` (≈7.4:1 on `#f97066`) or keep the fill at `#d92d20`. |
| `#ffffff` on `#a78bfa` | **2.72** ❌ | Landing `.step-number` — `background: var(--color-accent); color: var(--p-white)`; dark accent flips to violet-300 while the foreground stays white. Light passes (white on `#6d28d9` = 7.10). Fix: dark on-accent foreground `#101828`, or don't re-point accent under filled shapes. |

---

## 4. Other Wave 151 items re-checked

| Item | Verdict | Evidence |
|---|---|---|
| P2-10 `.visually-hidden` promoted | ✅ PASS | Defined once in BaseLayout; local api-keys copy gone |
| P2-10 `<caption>` on tables | ❌ FAIL | Still **zero** `<caption>` in the frontend |
| P2-10 `.table-wrap` keyboard-scrollable | ❌ FAIL | Still bare `overflow-x:auto`; no `tabindex`/`role="region"`/`aria-label` on any of the 13 instances |
| P2-11 Forked palettes collapsed | ✅ PASS | Login/register/forgot/landing contain **no colour literals**; all use the shared auth shell + tokens. Login's blue and register's teal buttons are gone — brand green everywhere (3.2.3/3.2.4 consistency ✅) |
| P2-12 Muted margin | ❌ FAIL (light) | `--color-muted` still `#667085`: 4.65 on bg, **4.45 on brand-soft** — unchanged. Dark muted passes with margin (§3) |
| P2-13 `lang` assurance | ✅ PASS (mostly) | Landing's Bangla footer line is wrapped in `<span lang="bn">`; login/register/checkout/pay/DashboardNav all call `applyI18n()`. Landing doesn't call it but renders no `data-i18n` strings. DashboardNav toggle keeps its bilingual `aria-label` with the visible label contained in the name (2.5.3 ✅) |
| Skip link / landmarks / reduced motion | ✅ PASS (held) | Unchanged and intact; `color-scheme` now `light dark` |

### Auth pages (login / register / forgot-password) — ✅ PASS overall

One `<h1>` each, `aria-labelledby` on the card, every input has a
`<label for>`, correct `autocomplete` (`email`, `current-password`,
`new-password`, `one-time-code`), errors as `role="alert"`, success as
`role="status"`, submit is a full 44 px `.btn btn-block`, focus lands on the
offending/TOTP field. Only gaps are the shared ones: no per-field
`aria-invalid` (P1-9) and hints other than the password hint remain
unassociated.

### New landing — heading structure & landmarks — ✅ PASS

Exactly one `<h1>` ("Payments that verify themselves"); sections use
`<h2 id="…-heading">` with matching `aria-labelledby` (gateways, features,
how, pricing); gateway regions, feature titles, and step titles are real
`<h3>`s in order — no skipped levels, no styling-only headings. Two labelled
landmarks beyond `<main id="main">`: `nav aria-label="Main"` and
`nav aria-label="Footer"`, plus `section aria-label="MiraclePay in numbers"`.
The hero verify-card is `aria-hidden="true"` — acceptable: it is a decorative
illustration of the adjacent copy and contains no focusable elements.
Eyebrow accent text passes both schemes (6.64 light / 6.88 dark). The one
landing failure is the dark step-number dot (§3). Advisory: header/footer
text links have no target height (see P1-6).

### Dialog & toast ARIA (`lib/ui.ts`, BaseLayout) — ⚠️ PARTIAL

- **Toast:** BaseLayout renders one persistent
  `<div class="toast-region" aria-live="polite">` and `toast()` appends plain
  `<div class="toast">` nodes — appended live-region children are announced,
  so success toasts work for AT ✅. **Error toasts go to the same polite
  region** with no `role="alert"` / assertive channel ❌ — a failed payment
  action is announced with the same urgency as "Copied to clipboard."
  Auto-dismiss at 5 s with no pause-on-hover/focus (advisory; content is
  non-essential, but error text vanishing is a real usability risk). Page
  adoption is also thin — no direct `toast(` calls found in page scripts.
- **Dialog:** `confirmAction()` uses a native `<dialog>` + `showModal()` —
  focus trapping, inert background, and Esc (via the `cancel` event, resolving
  `false`) are all correct by platform ✅. But: the message `<p>` is not tied
  to the dialog with `aria-describedby`, and there is no `aria-labelledby` /
  heading, so the dialog's accessible name is empty; **initial focus is placed
  on the destructive OK button** (`ok.focus()`, ui.ts:137) rather than Cancel —
  the unsafe default for irreversible actions; and the dialog element is
  appended to `document.body` and never removed, so repeated confirms stack
  dead dialogs in the DOM. **Adoption is zero:** no page imports
  `confirmAction`; `api-keys.astro:510` still calls `window.confirm()`.

### DashboardNav — ✅ PASS with one exception

Grouped pill nav keeps `nav aria-label="Dashboard"`, `aria-current="page"`,
admin items `hidden` until `/auth/me` resolves, group labels `aria-hidden`
(visual only — acceptable, groups are organisational). Links are now 44 px
(§2, P1-6). The exception is its own shell buttons: language toggle and
Log out are `.btn-sm` = 36 px. Account email is plain text (not a target) ✅.
Mobile nav is `overflow-x:auto` without a focusable region — same
`.table-wrap` caveat.

---

## 5. Remaining fixes, ranked

1. **Merchant brand-colour gate (P0, carried).** Validate in settings
   (compute vs `#ffffff`, warn/block < 4.5:1 with the ratio shown) **and**
   clamp at render in checkout/pay (darken, or switch label to `#101828`).
   Unchanged since Wave 151; still the top risk.
2. **Dark `.btn-danger` foreground (new, AA fail 2.79:1).** Dark-scheme
   `color: var(--p-gray-950)` on the `#f97066` fill (or pin the dark fill to
   `#d92d20`/white). One token-layer rule; affects every destructive action.
3. **Android `colorPrimary`/`colorSecondary` → `#0B7A55` (P0, carried).**
   One line in `themes.xml`; 3.39:1 → 5.34:1.
4. **Landing step-number dark pair (new, AA fail 2.72:1).** Dark on-accent
   foreground, or exclude filled accent shapes from the dark accent re-point.
5. **Wire the built helpers (P1, carried pattern).** Call `fieldError()` on
   login/register/checkout/pay validation; replace `window.confirm`
   (api-keys:510) with `confirmAction()` after fixing its focus-to-Cancel,
   `aria-describedby`, and post-close removal. Add `role="alert"` (or an
   assertive region) for error toasts.
6. **Name JS row actions (P1, carried).** `aria-label="Revoke key {name}"`
   etc. on every dynamically created Revoke/Copy/Send/Confirm/Cancel.
7. **Finish the `<h1>` demotion (P1, carried).** Checkout error `<h1>` →
   `<h2>`; pay error + "Payment started" `<h1>`s → `<h2>`.
8. **Confine `.btn-sm` or raise it (P1).** Either raise to 40–44 px, or
   restrict it to genuinely dense table rows — it should not style the nav
   shell or the payer-page language toggles (36 px today).
9. **Tables (P2, carried).** Add visually-hidden `<caption>`s; give
   `.table-wrap` `tabindex="0" role="region" aria-label`.
10. **Light muted margin (P2, carried).** Move `.field-hint`/`.stat-hint`/
    table `th` to `#475467` (7.69:1 white) or forbid muted on brand-soft
    (4.45:1 today).

*Review only — no source files were modified.*
