# 08 — Motion & Interaction Feedback Audit

**Role:** Motion/Interaction Designer, 10-agent design engineering team (Wave 151 audit)
**Scope inspected:** `frontend/src/layouts/BaseLayout.astro`, `frontend/src/components/DashboardNav.astro`,
`frontend/src/lib/api.ts`, all pages under `frontend/src/pages/` (landing, login, register,
dashboard ×8, admin ×2, `checkout/[id].astro`, `pay/[slug].astro`)
**Method:** source audit only — every `transition` / `animation` / `@keyframes` / loading /
disabled / confirm / polling / validation pattern grepped and read in context. No source edits.

---

## 1. Current state inventory

### 1.1 Headline finding: there is effectively no motion system

The entire frontend contains **exactly four `transition` declarations and zero `@keyframes`,
zero `animation`, zero spinners, and zero skeletons**:

| Location | Transition |
|---|---|
| `BaseLayout.astro` `.btn` | `background 0.15s ease` |
| `BaseLayout.astro` `.skip-link` | `top 0.15s ease` |
| `DashboardNav.astro` `.dashboard-nav-link` | `background 0.15s ease, color 0.15s ease` |
| `dashboard/gateways.astro` `.gateway-card:hover` | `border-color 0.15s ease, box-shadow 0.15s ease` |

Everything else is instantaneous: table-row hover, link underlines, panel show/hide,
badge/status changes, list re-renders. There is **no Astro `ClientRouter`/View Transitions**
(Astro 7.4.13, plain MPA), so every navigation is a full white-flash page reload.
The de-facto duration is `0.15s ease` everywhere it exists — a coincidence, not a token:
no `--motion-*` custom properties exist in `:root` alongside the colour/radius tokens.

### 1.2 What already works (keep and build on)

- **Double-submit protection is universal and correct.** Nearly every submit/action handler
  sets `button.disabled = true` before the fetch and restores it in `catch`/`finally`
  (login, register, checkout, pay, settings, gateways, api-keys, invoices, webhooks,
  transactions refund, admin, plugins). `.btn:disabled { opacity: 0.55 }` gives a visible
  (if static) in-flight state.
- **Semantic live regions are common.** Feedback uses `.alert-error` (`role="alert"`) and
  `.alert-success` (`role="status"`), so screen readers announce results without motion.
- **Reduced-motion baseline exists** (Wave 23): `BaseLayout.astro` collapses all
  transitions/animations to `0.01ms` under `prefers-reduced-motion: reduce`, and
  `pay/[slug].astro` explicitly checks `matchMedia` before smooth-scrolling to success.
- **Two good local patterns to promote to system patterns:**
  - *Copy feedback* (`payment-links.astro`): button label flips to `Copied` / `Copy failed`
    and reverts after 1600 ms — the only time-based feedback in the app.
  - *Inline destructive panel* (`transactions.astro` refund): an expanding row panel with
    amount/reason fields, inline error span, focus moved to the amount input on open,
    Cancel removes the panel. Far better than a modal for this job.
  - *Progressive disclosure with focus* (`login.astro`): a `totp_required` error reveals the
    2FA field and focuses it — the model for validation-driven reveals.

### 1.3 Feedback inventory per key action

Legend: **In-flight** = what the user sees while the request runs; **Success** / **Error** =
feedback after it resolves. "Static alert" = `.alert-*` paragraph unhidden in place,
no animation, never auto-dismisses.

| Page / action | In-flight | Success | Error | Gaps |
|---|---|---|---|---|
| **Login** (`login.astro`) | Button disabled only; label stays "Sign in" | Hard redirect `location.href = /dashboard` | Static alert; TOTP case reveals + focuses code field | No busy label/spinner; slow auth looks frozen |
| **Register** | Button disabled **+ label → "Creating account…"** ✅ | Redirect to `/dashboard` | Static alert; label restored | One of only 4 label-change patterns; not the norm |
| **Create merchant** (dashboard) | Submit disabled | **`window.location.reload()`** — full page flash, scroll/focus lost | Form-adjacent static alert (`cm-error`) | No inline append; reload is the crudest possible transition |
| **Create payment** (dashboard) | Submit disabled | Result panel unhidden + static success alert + checkout/redirect links | Static alert (`cp-error`) | Panel pops in; no scroll/focus move to result |
| **Dashboard load / 30 s poll** (`dashboard/index.astro`) | Text `Loading your dashboard…` once; **poll is silent** | Stats, chart (SVG `replaceChildren`), tx table (`replaceChildren`) swap instantly; small `Updated HH:MM:SS.` timestamp after analytics | Per-section static alerts (`chart-error`, `tx-error`, …) | No refresh indicator; numbers/rows change under the reader with no highlight; a poll mid-read causes content jump |
| **Merchant switcher** (dashboard, settings) | None — form hidden instantly, refetch silent (settings shows page loader) | Form/content unhidden | Page-level alert | No skeleton; layout collapses then re-expands (CLS) |
| **Transactions filter / load** | `Loading transactions…` text toggled `hidden` | Table re-rendered wholesale | Reuses one `#tx-message` alert for success **and** error | No skeleton rows; filter submit has no busy state on the button |
| **Refund** (transactions) | Confirm + Refund buttons disabled | Page-level success message + full list reload | Inline error span **inside the panel** ✅, buttons re-enabled | Panel opens/closes instantly; list reload loses the user's place |
| **Payment links: create** | Submit disabled | Page-level success alert + list reload; validation errors **focus the offending field** ✅ (title, amount, max-uses) | Form-level alert | Success alert sits at page top, far from the form on long pages |
| **Payment links: copy URL** | — | **Button label → "Copied", reverts in 1600 ms** ✅ | Label → "Copy failed" | Best feedback in the app; not shared with api-keys copy (below) |
| **API keys: create** | Submit disabled (both create + a stray second disable path) | One-time key box unhidden + `scrollIntoView({behavior:'smooth'})` | Form + page static alerts | Smooth scroll **ignores reduced-motion** (JS behaviour overrides the CSS rule — see §3) |
| **API keys: copy** | — | Persistent `Copied to clipboard.` status text | Silent (clipboard failure unhandled in the visible pattern) | Inconsistent with payment-links copy pattern |
| **API keys: revoke** (only destructive confirm in the app) | **`window.confirm()`** — native, unstyled, un-animated; then button disabled | List re-render | Page alert | No undo; native confirm breaks visual system; no row-level removal animation possible |
| **Invoices: create** | Submit disabled | Page-level success alert + list reload | Form alert | Line-item add/remove is instant; totals `<dl aria-live="polite">` ✅ is a good live-update precedent |
| **Invoices: send** | Button disabled **+ label → "Sending…"** ✅ | Row status presumably updates on reload | Row/page alert | Label pattern again exists but is not systematised |
| **Gateways: configure/save** | Save button disabled | `Settings saved.`-style success alert beside form ✅; panel `scrollIntoView` smooth | Form alert; success/error mutually hide each other ✅ | Config panel pops open instantly; smooth scroll ignores reduced-motion |
| **Webhooks: save URL** | Button disabled **+ status text → "Saving…"** ✅ | Page success alert + status text result ("…delivery is now disabled…" for empty URL) | Page alert | Two feedback channels (status text + alert) can diverge |
| **Webhooks: test signature / redeliver** | Button disabled | Result area (`aria-live="polite"`) | Form alert | No pending shimmer on result area |
| **Settings: save** | Submit disabled only | Persistent `Settings saved.` success alert beside form ✅ | Form alert | Success never dismisses — stale "saved" lingers after further edits (no dirty-state clearing) |
| **KYC submit / 2FA enable-disable** (dashboard) | Buttons disabled in flows | Dedicated `kyc-ok` / `twofa-ok` success alerts ✅ | Dedicated error alerts; `totp_invalid` mapped to friendly text ✅ | Panel reveals (2FA setup) are instant; QR/secret appearance has no focus move |
| **Admin: merchant status, settings, KYC decision** | Control disabled (`select.disabled`, submit disabled) | Inline note / `settings-saved` alert / queue re-render | Per-section alerts | Status change is instant with no row highlight; admin tables built via `innerHTML` swap |
| **Plugins: toggle / save config / run / logs** | Button disabled; status text → "Saving…" ✅; logs area → `Loading logs…` | Page success alert naming the plugin ✅; badge text flips Enabled/Disabled | Page alert / inline `Error: …` in output area | Badge flip is instant; run output appears with no pending indicator beyond disabled button |
| **Checkout load** (`checkout/[id].astro`) | `Loading checkout…` card (`aria-live="polite"`) | Card swaps in; status badge (`pending`/`Processing`/…) set by class change | Full-card error state (invalid/expired) ✅ | **No polling after load** — see §1.4 |
| **Checkout submit** | Submit disabled only | Form area replaced by `submitted-area` success alert; badge flips to `Processing` | Form alert; button re-enabled ✅ | **Dead end:** payer is told payment is processing but the page never checks again — verification completing elsewhere is invisible until manual reload |
| **Pay link load / pay** (`pay/[slug].astro`) | `Loading payment link…` card | Whole card swapped for success panel + **reduced-motion-aware smooth scroll** ✅ | Error card / form alert | Same no-polling dead end as checkout after payment starts |
| **Landing page** (`index.astro`) | n/a (static) | n/a | n/a | No entrance/scroll motion at all; hover underlines instant — acceptable, lowest priority |

### 1.4 The three structural gaps

1. **Checkout/pay never poll.** The dashboard polls every 30 s, but the two pages whose whole
   purpose is waiting for an asynchronous verification (SMS/gateway) do not poll at all.
   After submit, checkout shows a static "submitted/processing" alert forever. This is the
   highest-value motion/interaction fix in the product: a status page that visibly *waits*
   (poll + status progression) instead of silently stranding the payer.
2. **Loading = a sentence.** Every page's loading state is a muted text paragraph
   (`Loading X…`) toggled with `hidden`. No `aria-busy`, no skeleton, no spinner, no
   minimum-display or show-delay — so fast loads flash text, slow loads give no sense of
   progress, and content arrival causes full layout shift on every page.
3. **Feedback placement is inconsistent.** Success is reported in at least five different
   places (page-top alert, form-adjacent alert, dedicated ok-paragraph, button label,
   status text) and never dismisses. The same user learns a different feedback geography
   on every page. There is no toast/announcement system to unify non-critical feedback.

### 1.5 Form validation feedback

- **Almost every form is `novalidate`** (api-keys, gateways, dashboard ×5, invoices,
  payment-links, transactions filters, webhooks ×2, settings, checkout, pay, admin,
  login, register). Native bubbles are deliberately off — but nothing systematic replaced them.
- **Validation is submit-time only, and partial.** Hand-rolled checks cover a few required
  fields per form (checkout sender number + trx ID; pay amount/phone/email/gateway;
  payment-links title/amount/max-uses; dashboard merchant name; refund amount > 0).
  Everything else defers to a server error string.
- **Errors are a single form-level sentence** in `.alert-error`. There is **no per-field
  error text, no invalid-field styling, and no `aria-invalid` / `aria-describedby` error
  linkage anywhere** (the one `aria-describedby` in the app points at register's password
  *hint*). A server error like "email invalid" gives the user no pointer to the field.
- **Focus behaviour is split:** checkout, pay, payment-links and login-TOTP move focus to
  the offending/revealed field; login-empty, merchant-create and most dashboard forms do not
  move focus to the error or the field at all.
- **No live validation:** nothing validates on blur or clears an error on input. A corrected
  field keeps its stale error until the next submit re-runs the handler (which does clear
  the banner first — the one consistent behaviour).
- **Dirty state is invisible:** settings/gateway forms keep showing "saved" after the user
  edits again; there is no unsaved-changes indicator anywhere.

---

## 2. Proposed motion system

Design stance: this is a payments tool. Motion exists to (a) prove the system heard you,
(b) show where new information landed, (c) make waiting legible. Nothing decorative,
nothing over 400 ms, nothing that moves money-related numbers ambiguously.

### 2.1 Duration & easing tokens (add to `BaseLayout.astro :root`)

```css
:root {
  --motion-instant: 75ms;    /* hover, press, checkbox/badge flips */
  --motion-fast:    120ms;   /* fades, small reveals, toast out */
  --motion-base:    200ms;   /* panels, inline expands, toast in, row highlight */
  --motion-slow:    320ms;   /* page/view transitions, dialog, drawer */
  --motion-ease:        cubic-bezier(0.2, 0, 0, 1);   /* standard, most UI */
  --motion-ease-out:    cubic-bezier(0, 0, 0.2, 1);   /* entrances */
  --motion-ease-in:     cubic-bezier(0.4, 0, 1, 1);   /* exits */
}
```

Rules: the existing `0.15s ease` usages migrate to `var(--motion-fast) var(--motion-ease)`.
Colour/opacity may use `--motion-instant`; anything involving layout (height, position)
uses `--motion-base` or slower. Durations above are ceilings, not targets.

### 2.2 Where transitions belong

| Surface | Treatment | Token |
|---|---|---|
| **Page navigation** | Adopt Astro `ClientRouter` for dashboard/admin only: 200 ms cross-fade + 4 px rise on `<main>`; public checkout/pay/landing stay hard loads (shareable, must work cold). Preserve scroll on back-nav; never animate on first load. | `--motion-base`, ease-out |
| **Nav active state** | `DashboardNav` pill: animate a sliding active indicator (transform on a shared underline/pill) instead of instant background swap; link colour keeps existing fast transition. | `--motion-base` |
| **Modal / confirm** | Replace `window.confirm()` (api-key revoke) and grow a shared `<dialog class="modal">`: backdrop fade + panel fade/rise 8 px, focus trap (native), Esc/backdrop dismiss, return focus to invoker. Used for: revoke key, refund confirm (optional — inline panel stays default), plugin disable, admin status change for merchants. | `--motion-slow` in, `--motion-fast` out |
| **Toast** | New shared toast region (see §2.4) — the single home for non-critical success/info feedback currently scattered across page-top alerts. Slide-up + fade in, fade out. | `--motion-base` in, `--motion-fast` out |
| **Tabs / segmented / merchant switcher** | Content cross-fades; **skeleton first** (§2.3) if fetch > 150 ms; active tab indicator slides. No full-page loader text. | `--motion-fast` |
| **Inline panels** (gateway config, refund row, 2FA setup, new-key box) | Height/opacity expand (grid-template-rows or max-height technique) instead of instant `hidden` pop; focus moves into panel on open (refund already does), returns to trigger on close. | `--motion-base` |
| **Buttons** | Keep background transition; add `transform: scale(0.98)` press state (`--motion-instant`); in-flight state becomes **spinner + verb label** everywhere ("Saving…", "Sending…", "Creating…") — generalise the register/invoices/webhooks pattern into a `setBusy(btn, label)` helper in `lib/api.ts` consumers. Disabled-opacity alone is banned as the only in-flight signal. | instant/fast |
| **Tables / lists** | New or changed rows get a one-shot background flash (`--brand-soft` → transparent) so poll updates and post-action reloads show *what changed*; removed rows fade/collapse before DOM removal (revoke, invoice item remove). No FLIP/layout animation — payments data must never appear to move between rows. | `--motion-slow` flash |
| **Status badges** | On status change (checkout badge, plugin badge, invoice/transaction status): brief scale/colour pulse + the flash above; the badge is the canonical "something progressed" signal on polling pages. | `--motion-base` |
| **Charts** (dashboard analytics) | On poll refresh, redraw with a 200 ms opacity cross-fade of the SVG group; never animate bar heights on money data (implies values changing). | `--motion-base` |
| **Hover/focus** | All interactive hovers get at least the colour transition (table rows, links, cards); `:focus-visible` ring stays instant (never animate focus). | `--motion-instant` |

### 2.3 Skeleton & loading patterns per page type

Global rules first:

- **Show-delay 150 ms:** if data arrives faster, no loader ever appears (kills the
  current "Loading…" flash on warm loads). **Minimum display 300 ms** once shown.
- Every loading region gets `aria-busy="true"` and a visually hidden `role="status"`
  text equivalent ("Loading transactions"); skeletons themselves are `aria-hidden`.
- Skeletons are **grey blocks matching the real layout's geometry** (`--bg` blocks with a
  subtle pulse, §3 for reduced-motion variant) — the goal is zero layout shift on arrival.
- Page-level text loaders (`Loading X…`) are retired everywhere below.

| Page type | Pattern |
|---|---|
| **Dashboard overview** | Skeleton stat cards (label + value blocks ×4) + skeleton chart block + 5 skeleton table rows. On 30 s poll: **no skeleton at all** — keep current data, show a small "Updating…" spinner in the header next to the existing `Updated …` timestamp, then row-flash diffs. |
| **List pages** (transactions, invoices, payment-links, api-keys, webhooks, admin tables, plugins, KYC queue) | Skeleton rows matching column count (5 default) inside the real `<table>` shell so headers never move. Filters/headers render immediately and stay interactive-disabled only where unsafe. |
| **Form pages** (settings, gateways, webhook config) | Form-shaped skeleton (label + input blocks) — or better, render the real form immediately with values filled when loaded (settings already loads fast); skeleton only for the merchant-dependent sections. Save never blocked by a page loader. |
| **Auth** (login, register) | No initial loader (static forms). In-flight = busy button only. |
| **Checkout / pay (public)** | Two-phase: (1) amount/merchant header skeleton resolves **first** — the amount is the trust anchor and must never flash a placeholder value; (2) form skeleton below. After submit: dedicated **status view** (see §2.4) with polling, not a static alert. |
| **Detail / result panels** (create-payment result, plugin logs/run output) | Inline skeleton block inside the panel area; panel itself expands per §2.2. |
| **Landing** | No loaders; optional one-shot fade-rise on hero only, disabled under reduced-motion. |

### 2.4 Optimistic feedback & error recovery

**Feedback architecture (target):**

- **Toast region** — one fixed bottom-right (bottom-centre on mobile) stack, max 3,
  `aria-live="polite"` for success/info, separate `role="alert"` path for errors.
  Success toasts auto-dismiss in **5 s** (pause on hover/focus); errors **persist** until
  dismissed or resolved. Every toast may carry one action: **Undo** or **Retry**.
  This replaces page-top success alerts (invoices, payment-links, webhooks, plugins,
  transactions) — errors that belong to a form stay with the form (below).
- **Form errors stay inline:** per-field error text under the input, linked with
  `aria-describedby`, field gets `aria-invalid="true"` + danger border; a form-level
  summary appears only when >1 field fails and receives focus on submit. Validation runs
  **on submit, then live on blur/input for already-invalid fields** (clear-as-you-fix).
  The existing focus-the-field pages (checkout, pay, payment-links) become the norm.
- **Busy helper:** `setBusy(button, "Saving…")` swaps in spinner + label, sets
  `aria-disabled`/disabled, restores on settle — one implementation, every page.

**Optimism policy — by action risk:**

| Action class | Policy |
|---|---|
| **Safe to do optimistically** (local, reversible): copy-to-clipboard, nav/tab switches, merchant switcher (render cached data, then refresh), plugin enable/disable toggle, gateway enabled toggle, invoice line-item add/remove, dismissals | Apply instantly, show pending tint on the affected row/control; on failure roll back and toast error with **Retry**. (Plugins toggle is already pessimistic-correct; optimism is optional polish there, not a fix.) |
| **Create, then confirm** (payment link, invoice, API key, merchant): | Insert a pending row/card at the top of the list with skeleton/pending badge the moment the form submits; replace it in place (flash) on success; on failure keep the form **filled**, mark the pending item failed with Retry/Discard. Merchant create must stop doing `location.reload()`. |
| **Never optimistic** (money movement / irreversible): checkout submit, pay-link pay, refund, API-key revoke, KYC decision, invoice send, settings that change payout behaviour | Pessimistic: busy state → server verdict → success state. Revoke additionally keeps a **5-second Undo toast** after success (re-issue path) since the action itself can't be un-done server-side. |
| **Settings saves** | Pessimistic save, but add **dirty-state tracking**: any edit after save clears the "Settings saved." alert and shows a subtle "Unsaved changes" dot/label on the save button; success toast replaces the persistent alert. |

**Error recovery patterns (currently almost absent — most page errors require a manual reload):**

1. **Retry in place:** every page-level `.alert-error` gains a Retry button that re-runs
   that section's loader (dashboard sections, admin sections, list pages) — no full reload.
2. **Preserve work:** on any failed submit, form values, open panels and scroll position
   survive (mostly true today — make it a tested rule, incl. refund panel and gateway config).
3. **Polling failure is quiet:** dashboard poll failures show the small header indicator as
   "Update failed — retrying" and back off (30 s → 60 s → 120 s cap); never a modal/alert
   for a background failure. Manual Retry in the header.
4. **Checkout/pay status polling (the P0 fix):** after submit, poll checkout status every
   3 s for the first minute, then every 10 s to a 10-minute cap, with `document.hidden`
   pause (mirror the dashboard guard). Status view shows a stepped progression
   (Submitted → Processing → Verified/Completed) with the badge pulse on each advance;
   terminal states replace the view (success check state / failed with reason + "try again"
   that restores the pre-filled form). Expiry of polling ends with a calm "still processing —
   you can safely close this page" message, not an error.
5. **Network vs server errors:** `apiFetch` failures distinguish offline/timeout
   ("You're offline — we'll retry") from server errors (show server message verbatim,
   as today). Toast for the former, inline for the latter.
6. **Focus & announcement on recovery:** when an async error lands, focus is **not** stolen
   (except submit-validation, § above); `role="alert"` announces it. When a user-initiated
   Retry succeeds, toast confirms recovery.

### 2.5 Priority order

- **P0:** checkout/pay status polling + status view; busy-button helper (spinner + verb)
  rolled to all submit buttons; per-field validation + `aria-invalid` on checkout/pay/auth.
- **P1:** skeleton system for dashboard + list pages; toast region replacing page-top
  success alerts; Retry buttons on page/section errors; dashboard poll indicator + row flash;
  kill `location.reload()` on merchant create.
- **P2:** `<dialog>` confirm replacing `window.confirm()`; panel expand animations;
  dashboard ClientRouter transitions; dirty-state indicators; landing entrance motion.

---

## 3. Reduced-motion rules

Existing foundation (keep): the Wave 23 global rule in `BaseLayout.astro` collapsing
`transition-duration`/`animation-duration` to `0.01ms` and `scroll-behavior: auto` under
`@media (prefers-reduced-motion: reduce)`. The rules below close its gaps and govern all
new motion from §2.

1. **The media query is the single switch.** All §2 motion must be implemented so the
   existing global rule neutralises it — CSS transitions/animations only, no WAAPI or
   JS-driven animation that bypasses it. Any exception needs a named justification here.
2. **JS scrolling must check explicitly — current bug class.** `behavior: 'smooth'` passed
   to `scrollIntoView` is **not** overridden by the CSS `scroll-behavior` rule. Today
   `pay/[slug].astro` checks `matchMedia('(prefers-reduced-motion: reduce)')` but
   `api-keys.astro` (new-key box) and `gateways.astro` (config panel) smooth-scroll
   unconditionally. Rule: all programmatic scrolling goes through one helper
   (`scrollToEl(el)`) that resolves behaviour from `matchMedia` at call time.
3. **Loading indicators are information, not decoration.** Under reduced motion: skeleton
   shimmer/pulse is disabled (static grey blocks), spinners become a static
   "Loading…/Saving…" text label (the busy-button label already carries the meaning) —
   never a rotating element for users who asked for no motion.
4. **State change must remain perceivable without motion.** Row flash, badge pulse and
   toast slide are enhancements only: under reduced motion the same events are conveyed
   by the text/badge change itself plus the live-region announcement. No information may
   be carried by movement alone (position, direction, speed) in any theme.
5. **View/page transitions:** under reduced motion the dashboard ClientRouter transition
   becomes an instant swap (no fade/rise). Dialogs/toasts appear and disappear instantly
   (opacity may still cross-fade ≤ `--motion-fast` if the global rule is relaxed for
   opacity-only in a future wave — default: instant).
6. **Polling & auto-updates:** reduced motion does not stop polling (it is not motion),
   but poll-driven changes must not flash/pulse; the `Updated …` text and live-region
   summary are the reduced-motion channel. Nothing on any page may auto-scroll under
   reduced motion.
7. **No new categories, ever, without an audit note:** parallax, auto-playing/carousel
   motion, animated backgrounds, chart draw-on animations, and number count-ups on money
   values are prohibited in this product regardless of the media query.
8. **Testing gate:** every wave touching §2 surfaces ships with a reduced-motion pass —
   OS setting on, walk: login → dashboard poll cycle → settings save → revoke dialog →
   checkout submit-to-verified — verifying no smooth scroll, no shimmer, no slide,
   and that every state change is still announced and visible as text.

---

*End of audit 08 — Motion/Interaction. Companion audits: design tokens/components,
dashboard UI, checkout UX, accessibility (owns the WCAG motion criteria referenced in §3).*
