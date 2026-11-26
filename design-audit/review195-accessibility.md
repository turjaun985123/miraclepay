# Review 195 — Accessibility Re-Review #3 (Wave 195, Design Review #3)

**Role:** Accessibility Engineer. **Scope re-checked (source only, no edits):**
every item in `design-audit/review165-accessibility.md` (§1–§5) against current
code at HEAD `def77d8` (Waves 189–194) plus the working tree — `frontend/src/styles/tokens.css`,
`frontend/src/layouts/BaseLayout.astro`, `frontend/src/lib/ui.ts` (`brandAccent`,
`fieldError`, `confirmAction`, toast), `frontend/src/pages/checkout/[id].astro`,
`frontend/src/pages/pay/[slug].astro`, `frontend/src/pages/dashboard/settings.astro`,
`frontend/src/pages/dashboard/api-keys.astro`, `frontend/src/pages/index.astro`,
and Android `res/values/colors.xml`, `res/values-night/colors.xml`,
`res/values/themes.xml` (+ `values-night/themes.xml`).
**Standard:** WCAG 2.2 AA. **Method:** static source review + contrast computed
by the reviewer from the hex values (WCAG relative-luminance formula, 2 dp).
No browser, screen-reader, TalkBack, or zoom run — **[verify live]** items carried.
**Working-tree caveat:** the tree was dirty during review (uncommitted changes to
`BaseLayout.astro`, `admin/index.astro`, `webhooks.astro`, `forgot-password.astro`
from parallel waves). One item below (§2.9, `<caption>`) exists **only** in that
uncommitted tree, not at HEAD — flagged where it matters.

**Headline: the two dark-mode failures from Review 165 are fixed, Android is
fixed, and the merchant brand gate finally exists — but the gate is set to the
wrong threshold (3:1 for normal-size button text) and is scheme-blind, so the
165 P0 is only PARTIAL. Of the 10 ranked fixes in Review 165 §5: 3 PASS,
4 PARTIAL, 3 FAIL. One new defect was introduced while wiring a 165 fix (§3, N1).**

---

## 1. The four re-checks named for this review

### 1.1 Dark `.btn-danger` — ✅ PASS (Review 165 §3 failure closed)

`BaseLayout.astro:168-172` now overrides the foreground in both dark entry
points (`@media (prefers-color-scheme: dark)` with `:not([data-theme="light"])`,
and `:root[data-theme="dark"]`): `.btn-danger { color: var(--p-gray-950) }`
(`#0b1220`) on the dark fills. Computed:

| Pair (dark) | Was (165) | Now | AA ≥4.5:1 |
|---|---|---|---|
| `#0b1220` on `--color-danger` `#f97066` | white = 2.79 ❌ | **6.72** | ✅ |
| `#0b1220` on `--color-danger-hover` `#f04438` | white = 3.76 ❌ | **4.98** | ✅ (thin margin) |

Light is unchanged and passing: white on `#d92d20` = **4.83**, hover white on
`#b42318` = **6.57**. Caveat: the override is a BaseLayout page-level rule, not
a token (`--color-on-danger` does not exist) — any future surface that styles a
danger button outside BaseLayout re-inherits the failure. Hover margin (4.98)
should not be spent by darkening the hover fill.

### 1.2 Dark `.step-number` — ✅ PASS (Review 165 §3 failure closed)

Same override block (`BaseLayout.astro:169,172`): `.step-number` foreground
becomes `#0b1220` in dark. The only `.step-number` is the landing step dot
(`index.astro:181`, `background: var(--color-accent); color: var(--p-white)`,
`aria-hidden="true"` — decorative numeral, but the fix is still correct):

| Pair | Was (165) | Now | Verdict |
|---|---|---|---|
| `#0b1220` on dark accent `#a78bfa` | white = 2.72 ❌ | **6.88** | ✅ |
| Light: white on `#6d28d9` (unchanged) | 7.10 | 7.10 | ✅ |

Same caveat as §1.1: page-level override, no `--color-on-accent` token; the
checkout `.steps .step-dot` does **not** use `.step-number` and relies on
`--color-on-brand` instead — which matters for §1.4.

### 1.3 `.table-wrap` focusability — ✅ PASS, with naming caveats

Fixed exactly as prescribed, and committed at HEAD: a BaseLayout script
(`BaseLayout.astro:229-234`) sets on every `.table-wrap`:
`tabindex="0"`, `role="region"`, and `aria-label="Scrollable table"` if unnamed.
All 13 instances in the frontend are static markup (api-keys 1, invoices 1,
payment-links 1, webhooks 3, transactions 1, dashboard/index 1, admin 5), so
the once-at-load script reaches all of them; no `.table-wrap` is created
dynamically. Caveats, none blocking the PASS:

- **All 13 regions share one identical name** — "Scrollable table" does not
  distinguish Transactions from Webhook deliveries (2.4.6/4.1.2 naming intent
  only half met); 13 extra `region` landmarks is also landmark noise. Prefer a
  per-instance label from the section heading (the same trick the caption
  patch in §2.9 uses) or drop `role="region"` where a label can't be unique.
- `tabindex="0"` is applied whether or not the wrap actually overflows —
  harmless, but adds tab stops on wide screens. Acceptable.
- Still **not** covered: DashboardNav's mobile `overflow-x:auto` nav (not a
  `.table-wrap`) — carried from 165, unchanged.

### 1.4 Merchant brand gate, checkout/pay (`lib/ui.ts brandAccent`) — ⚠️ PARTIAL

The unbounded failure from 151/165 is closed in shape but not in strength.
`ui.ts:186-190`: `brandAccent()` validates `#rrggbb` and returns the colour
only when white-on-it contrast ≥ 3:1, else `null` → platform default. Both
payer pages now use it — `checkout/[id].astro:680-684` and
`pay/[slug].astro:476-480` set `--color-brand` on `documentElement` and the
submit button's inline `background`. The old worst case is gone (white on
`#ffd60a` = 1.41 can no longer render). Three defects keep this at PARTIAL:

1. **Threshold is 3:1; the button text needs 4.5:1.** The code comment claims
   "large/bold control text", but `.btn` text is `--text-body` = 0.9375 rem
   (≈15 px) at weight 600 — not WCAG "large" (≥24 px, or ≥18.66 px bold).
   Every merchant colour in the 3.00–4.49 band passes the gate and fails
   1.4.3 on the "Pay now" label: e.g. `#949494` passes at white = **3.03**,
   `#8a8a8a` at **3.45**. The gate must be ≥4.5:1 (e.g. `#767676` = 4.54 is
   the lightest passing grey), or the colour must be darkened until it is.
2. **The gate is scheme-blind — it tests white, but dark mode paints ink.**
   `.btn` foreground is `var(--color-on-brand)`: white in light, but
   `#06231a` in dark. In dark, a merchant colour is applied with the dark-ink
   foreground the gate never computed: the platform's own `#0b7a55` gives
   `#06231a`-on-it = **3.11** ❌; `#767676` (white 4.54) gives **3.66** ❌.
   The same pair drives `.steps` current-step dots (`--color-brand` bg +
   on-brand fg). Gate must compute against the *active scheme's* on-brand
   colour, or force the button's foreground to white when a merchant accent
   is applied.
3. **Settings side is still silent.** `dashboard/settings.astro:227-244`
   still syncs the colour/hex fields with **no** ratio computation, warning,
   or preview (grep: no `brandAccent`/`contrastRatio` import — despite
   `ui.ts` now exporting `contrastRatio`). A merchant whose colour will be
   discarded is never told; the payer page silently falls back. Server-side
   validation (`crates/domain/src/settings.rs:157-160`) still checks format
   only. Also minor: the submit button's inline `background` defeats
   `.btn:hover` (`--color-brand-hover` is never re-pointed), so the themed
   button has no hover state.

Verdict: the catastrophic case is fixed; a bounded AA failure (3.00–4.49 in
light, worse pairings in dark) remains merchant-reachable. Still the top item.

---

## 2. Item-by-item vs the Review 165 list

### §5 ranked list from Review 165

| # | 165 item | 195 verdict | Evidence |
|---|---|---|---|
| 1 | Merchant brand gate (P0) | ⚠️ PARTIAL | §1.4 — gate exists at 3:1, scheme-blind, settings silent |
| 2 | Dark `.btn-danger` fg (2.79) | ✅ PASS | §1.1 — 6.72 / hover 4.98 |
| 3 | Android `colorPrimary` → `#0B7A55` (P0) | ✅ PASS | §2.10 — white on `#0B7A55` = **5.34** (was 3.39) |
| 4 | Landing step-number dark (2.72) | ✅ PASS | §1.2 — 6.88 |
| 5 | Wire built helpers (`fieldError`, `confirmAction`, error toasts) | ⚠️ PARTIAL | §2.8, §3 N1 — payer fields wired; auth/dashboard not; `confirmAction` adopted with a broken call signature; error toasts still polite-only |
| 6 | Name JS row actions (P1) | ❌ FAIL, unchanged | `api-keys.astro:478` still bare `revokeButton.textContent = 'Revoke'`; grep still finds **zero** JS-set `aria-label`s on row actions across transactions/invoices/webhooks/payment-links/api-keys |
| 7 | Finish `<h1>` demotion (P1) | ❌ FAIL, unchanged | checkout still **2** `<h1>`s (`[id].astro:55` error, `:75` `#merchant-name`); pay still **3** (`[slug].astro:36` error, `:47` `#pay-title`, `:125` "Payment started") |
| 8 | Confine/raise `.btn-sm` (36 px) | ❌ FAIL, unchanged | `.btn-sm, .btn-small { min-height: 2.25rem }` untouched (`BaseLayout.astro:95`); still styles DashboardNav language toggle + Log out (`DashboardNav.astro:42-43`) and the checkout language toggle (`[id].astro:36`, `.btn-small`). Still passes 2.5.8 AA (≥24 px), fails the system's own 44 px floor / 2.5.5 AAA |
| 9a | `.table-wrap` focusable region | ✅ PASS | §1.3 |
| 9b | `<caption>` on tables | ⚠️ PARTIAL | §2.9 — static markup still zero; uncommitted runtime injection only |
| 10 | Light muted margin (P2) | ❌ FAIL, unchanged — with a concrete victim now | §2.11 — token unchanged, 4.45:1, and the checked gateway card puts muted hint text on brand-soft |

### Other 165 items re-checked

| 165 item | 195 verdict | Evidence |
|---|---|---|
| P0-1 `--border-strong` (1.4.11) | ✅ PASS (held) | `tokens.css` unchanged: `#7d879c` = 3.61 on white / 3.37 on bg; dark `#98a2b3` = 6.89/7.27 (165 computations stand, hexes identical) |
| P0-4 `--danger-soft` text pair | ✅ PASS (held) | Tokens unchanged: `#b42318` on `#fef3f2` = 6.05; dark `#f97066` on `#3a1512` = 5.82 |
| P1-5 Input focus indicator | ✅ PASS (held) | `.input:focus` 2 px `--color-focus` outline unchanged; light 5.34, dark 8.36+ |
| P2-10 `.visually-hidden` promoted | ✅ PASS (held) | Single definition in BaseLayout; the new caption patch (§2.9) correctly reuses it |
| P2-11 Forked palettes collapsed | ✅ PASS (held) | No colour literals reintroduced in auth/landing pages |
| P2-13 `lang` assurance | ✅ PASS (held) | No regression found; landing Bangla line still `lang="bn"` |
| Skip link / landmarks / reduced motion | ✅ PASS (held) | Intact in BaseLayout |
| Dialog & toast ARIA | ⚠️ PARTIAL (unchanged + N1) | `confirmAction` (`ui.ts:115-140`) still: no `aria-labelledby`/`aria-describedby`, initial focus on the destructive OK button (`ok.focus()`), dialog appended and never removed. Toast region still one `aria-live="polite"` for success **and** error — no `role="alert"`/assertive channel for error toasts |

### 2.8 Per-field `aria-invalid` adoption (165 P1-9) — ⚠️ PARTIAL, real progress

The "built but never used" pattern is broken on the payer pages:

- **Checkout — ✅ for its form:** both fields wired (`[id].astro:798,803`):
  sender number and transaction ID call `fieldError()` (sets
  `aria-invalid="true"` + `.field-error` + `aria-describedby`), receive
  `.focus()`, and clear on `input` (`:836-837`).
- **Pay — ⚠️ partial:** phone (`[slug].astro:547`) and email (`:555`) wired
  with focus, as above. But the **amount** error still goes only through
  page-level `showFormError()` (`:537`) — despite a `clearFieldError(amountInput)`
  listener at `:599` that clears an error no code path ever sets (dead wiring).
  Gateway-not-chosen is also page-level only (`:562`); the radio group gets no
  `aria-invalid`/group-level description.
- **Everywhere else — ❌ zero adoption:** grep finds no `fieldError`/
  `aria-invalid` in login, register, forgot-password, settings, or any
  dashboard form. Login still relies solely on `#login-error[role=alert]`.
  The credential forms — half the 165 complaint — are untouched.
- **Helper defect to fix before wider rollout:** `fieldError()` *overwrites*
  `aria-describedby` with the error id, silently dropping any pre-existing
  hint association (register's `password` → `password-hint` would be lost the
  moment register adopts it); `clearFieldError()` removes the error node but
  leaves `aria-describedby` pointing at the removed id and never restores the
  hint. Harmless on today's checkout/pay fields (no prior `aria-describedby`),
  a regression waiting on the auth forms.

### 2.9 `<caption>` absence (165 P2-10) — ⚠️ PARTIAL (working tree only)

Static markup at HEAD: still **zero** `<caption>` in the frontend (grep count 0).
The uncommitted working tree adds a BaseLayout runtime patch (labelled
"Review 195 a11y") that prepends a `visually-hidden` `<caption>` to every
`table.table`, taking its text from the closest section/card heading, falling
back to "Data table". All 13 tables carry `class="table"`, so coverage would
be complete **if committed and if JS runs**. Why only PARTIAL: (a) it is not
in HEAD `def77d8` — a reset/checkout of the tree loses the fix; (b) no-JS /
pre-script assistive tech and any server-rendered snapshot see no caption —
source-level `<caption>` remains the robust form; (c) multi-table sections
risk duplicate or wrong-heading captions **[verify live]** (webhooks has 3
tables; admin has 5 — each sits in its own section/card today, but the
`closest('section, .card')` + first-heading heuristic should be checked per
table once rendered).

### 2.10 Android — ✅ PASS on contrast (light + dark roles), minor role gaps

Waves 189–194 replaced the M2 theme wholesale: `themes.xml` is now
`Theme.Material3.DayNight.NoActionBar` with all roles sourced from
`colors.xml` / `values-night/colors.xml`; the old hard-coded `#0E9F6E`
(3.39:1) is gone. Computed from the hex values:

**Light (`values/colors.xml`):**

| Pair | Ratio | AA |
|---|---|---|
| `md_on_primary` `#FFFFFF` on `md_primary` `#0B7A55` | **5.34** | ✅ (was 3.39 ❌) |
| `md_secondary` `#4C6358` on white / on surface `#F7FAF8` | 6.50 / 6.18 | ✅ |
| `md_on_primary_container` `#002114` on `#A6F2D0` | 13.22 | ✅ |
| `md_on_secondary_container` `#0A1F16` on `#CFE9DA` | 13.37 | ✅ |
| `md_error` `#BA1A1A` on white; `md_on_error` white on it | 6.46 / 6.46 | ✅ |
| `md_on_error_container` `#410002` on `#FFDAD6` | 13.26 | ✅ |
| `md_on_surface` `#191C1B` on `md_surface` `#F7FAF8` | 16.34 | ✅ |
| `md_on_surface_variant` `#404943` on surface / on `#DBE5DE` | 8.87 / 7.23 | ✅ |
| `md_outline` `#707973` on surface (non-text, ≥3:1) | 4.28 | ✅ |
| `md_outline_variant` `#C0C9C2` on surface | 1.61 | advisory only — M3 decorative divider role, never a sole control boundary **[verify live]** |
| `md_tertiary` `#3D6373` as text on white | 6.50 | ✅ |

**Dark (`values-night/colors.xml`):**

| Pair | Ratio | AA |
|---|---|---|
| `md_on_primary` `#003824` on `md_primary` `#7ED7AE` | **7.68** | ✅ |
| `md_on_primary_container` `#A6F2D0` on `#005236` | 7.18 | ✅ |
| `md_on_secondary` `#1E352A` on `#B3CCBE` | 7.70 | ✅ |
| `md_on_secondary_container` `#CFE9DA` on `#334B3E` | 7.36 | ✅ |
| `md_on_error` `#690005` on `md_error` `#FFB4AB` | 7.72 | ✅ |
| `md_on_error_container` `#FFDAD6` on `#93000A` | 7.24 | ✅ |
| `md_on_surface` `#E0E3E1` on `md_surface` `#101413` | 14.36 | ✅ |
| `md_on_surface_variant` `#C0C9C2` on surface / on `#404943` | 10.94 / 5.49 | ✅ |
| `md_primary` `#7ED7AE` / `md_error` `#FFB4AB` as text on surface | 10.80 / 10.93 | ✅ |
| `md_outline` `#8A938C` on surface (non-text) | 5.86 | ✅ |
| `md_outline_variant` `#404943` on surface | 1.99 | advisory only, as light |
| Inverse pair (both schemes) `#EFF1EF` ↔ `#2D3130` | 11.61 | ✅ |

Status-bar handling is consistent (`windowLightStatusBar` true in light,
false in `values-night`). Remaining Android gaps (none are contrast failures):
**no `md_on_tertiary` / `md_on_tertiary_container` in either scheme and
`themes.xml` never sets `colorOnTertiary`/`colorOnTertiaryContainer`** — M3
framework defaults fill in, so any tertiary-container UI (e.g. the outbox
avatar uses `colorOnPrimaryContainer` today, safe) is themed by default, not
by token; tertiary-as-text passes if used (above). Carried from 151/165,
still true: **no `accessibilityLiveRegion` or `accessibilityHeading` in any
layout, and no TalkBack semantics work in the Java layer** (grep: only one
`contentDescription`, in `activity_settings.xml`) — the outbox list's
Waiting/Failed status changes have no live-region announcement. **[verify live
with TalkBack]**

### 2.11 Muted on brand-soft (165 P2-12) — ❌ FAIL, unchanged, now with a live victim

`--color-muted` is still `#667085` (`tokens.css`, unchanged). Recomputed:
**4.97** on white, **4.65** on `--color-bg` `#f6f7fb`, **4.45 on
`--color-brand-soft` `#e6f6ef`** — byte-identical to 165, still 0.05 under AA
for normal text. In 165 this was a margin warning; it is now a concrete
co-location: the payer gateway radio cards — `.gateway-card:has(input:checked)`
gets `background: var(--color-brand-soft)` (`BaseLayout.astro:181`) while its
`.gateway-hint` child stays `color: var(--color-muted)` at `--text-sm`
(0.8125 rem) — **muted small text on brand-soft = 4.45:1 on the selected
gateway card, on both payer pages, in the default state a payer ends in.**
Dark is fine (muted `#98a2b3` on dark brand-soft `#0c2b21` = **5.90**).
One-line fixes, either: move `.gateway-hint` (and `.field-hint`/`.stat-hint`/
`th`) to `--p-gray-600` `#475467` (= **6.88** on brand-soft), or darken the
hint inside a checked card to `--color-brand-hover` (as `.badge-success` and
`.alert-success` already do on the same soft background).

---

## 3. New issues found in this review

- **N1 (high) — `confirmAction` adopted with the wrong signature; the dialog
  now renders garbage.** `api-keys.astro:510-517` (the 165 fix for
  `window.confirm`) calls `confirmAction({ title, body, confirmLabel, danger })`
  with an **object**, but `ui.ts:115` still declares
  `confirmAction(message: string, confirmLabel = "Confirm")`. At runtime the
  message paragraph's `textContent` becomes `"[object Object]"`, the button
  reads "Confirm" instead of "Revoke key", and title/body/danger are silently
  dropped — for an irreversible destructive action, announced to AT as an
  unlabelled dialog containing "[object Object]". Reconcile the two sides
  (the object form is the better API: it can also carry the
  `aria-labelledby`/`aria-describedby` wiring and focus-Cancel default that
  165 asked for and `ui.ts` still lacks).
- **N2 (medium) — brand gate dark-scheme pairing** (see §1.4.2): gate tests
  white; dark paints `#06231a`. Any merchant accent applied in dark is an
  uncomputed pair; platform green itself fails there (3.11).
- **N3 (medium) — `fieldError` describedby handling** (see §2.8): overwrites
  existing hint associations; `clearFieldError` leaves a dangling
  `aria-describedby`. Fix in the helper before auth forms adopt it: merge ids
  on set, restore the original set on clear.
- **N4 (low) — identical generic names at scale:** 13 × `role="region"
  aria-label="Scrollable table"` (§1.3) plus caption fallback "Data table"
  (§2.9) — pages gain many indistinguishable landmarks/captions. Derive both
  from the section heading, as the caption patch already does for captions.
- **N5 (low) — Android tertiary on-roles undefined** (§2.10): add
  `md_on_tertiary` / `md_on_tertiary_container` (both schemes) and wire
  `colorOnTertiary` / `colorOnTertiaryContainer` in `themes.xml`, or tertiary
  UI inherits framework defaults off-token.
- **N6 (low) — dark danger/step fixes live as layout overrides, not tokens**
  (§1.1–1.2): introduce `--color-on-danger` / `--color-on-accent` semantic
  tokens so non-BaseLayout surfaces can't re-inherit the 165 failures.

---

## 4. Ranked final-pass fix list — Waves 196–199

1. **Fix the brand gate properly (P0, carried twice).** Raise `brandAccent`
   to ≥4.5:1 against the *rendered* foreground — compute both schemes
   (white in light, `#06231a` in dark) or pin the themed button's foreground;
   also re-point `--color-brand-hover` (or drop the inline background so
   hover works). Add the settings-side warning using the already-exported
   `contrastRatio()`: show the computed ratio and state plainly when a colour
   will fall back to the platform green.
2. **Reconcile `confirmAction` (N1) and finish the dialog.** One signature
   (object form), `aria-labelledby` from the title, `aria-describedby` from
   the body, initial focus on **Cancel** for `danger: true`, remove the
   dialog element on close. Add an assertive channel (`role="alert"`) for
   error toasts in the same wave.
3. **Finish per-field errors.** Fix the `fieldError` describedby merge/restore
   (N3), then wire login, register, forgot-password, and settings; give pay's
   amount field a real `fieldError` (its clear-listener is already waiting)
   and the gateway radio group a group-level error association.
4. **Muted on brand-soft (one line).** `.gateway-hint` inside a checked card —
   and `.field-hint` / `.stat-hint` / `.table th` generally — to `#475467`
   (6.88 on brand-soft), or forbid `--color-muted` text on
   `--color-brand-soft` in the token lint.
5. **Land the table semantics in source.** Commit the caption injection (or
   preferably write real `<caption class="visually-hidden">`s into the 13
   tables) and give each `.table-wrap` a unique region label from its
   heading; extend the same treatment to the DashboardNav mobile scroller.
6. **Name the JS row actions.** `aria-label="Revoke key {name}"` pattern on
   every dynamically created Revoke/Copy/Send/Confirm/Cancel across
   api-keys, transactions, invoices, webhooks, payment-links.
7. **Finish the `<h1>` demotion.** Checkout error `<h1>` → `<h2>`; pay error
   and "Payment started" `<h1>`s → `<h2>` (one line each, third review running).
8. **Resolve `.btn-sm`.** Raise to 44 px, or confine it to dense table rows
   and return the DashboardNav shell buttons and payer language toggles to
   the standard `.btn` size.
9. **Tokenise the dark filled-shape foregrounds (N6)** —
   `--color-on-danger`, `--color-on-accent` — and complete Android's
   tertiary on-roles (N5); add `accessibilityLiveRegion` to the Android
   outbox status and `accessibilityHeading` to activity titles, then a
   TalkBack pass **[verify live]**.

*Review only — no source files were modified. All ratios above were computed
by the reviewer from the hex values in the current tree.*
