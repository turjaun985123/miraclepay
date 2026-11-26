# Review 180 — Motion & Interaction Feedback, Design Review #2

**Role:** Motion/Interaction Designer · **Scope:** review only, no source edits
**Inspected:** `frontend/src/styles/tokens.css`, `frontend/src/layouts/BaseLayout.astro`,
`frontend/src/lib/ui.ts` + every call site (grep over `frontend/src/pages/**`,
`components/**`), payer pages `checkout/[id].astro` / `pay/[slug].astro`, dashboard
async paths (`dashboard/index.astro` 30 s poll, all list/form pages, `admin/*`).
**Baseline:** `design-audit/08-motion.md` (Wave 151: 4 transitions, 0 keyframes/spinners/
skeletons, no `--motion-*`, checkout/pay never poll, loading = a sentence, no toast).

---

## 1. Verdict

The Wave 151 system now **exists as infrastructure** — tokens, toast, skeleton, spinner,
dialog, busy/field-error/scroll helpers are all built — but **adoption is the failure
mode of this wave**: the two highest-value helpers (`withBusy`, `skeletonAfter`) have
**zero call sites**, `toast` has **zero direct call sites**, and most pages still run
the 151 patterns (text loaders, disabled-only buttons, page-top alerts, instant
`hidden` swaps). Biggest regression-risk finding: the one `confirmAction` adopter
calls it with an incompatible signature (§4, R1). Biggest user-facing gap: checkout
polling exists but **is never started by the submit flow itself** (§3, G1).

---

## 2. Passes (verified in source)

| # | Pass | Evidence |
|---|---|---|
| P1 | **Motion tokens landed, values as spec'd.** `--motion-instant/fast/base/slow` = 75/120/200/320 ms in `tokens.css:85`. Naming diverged from 08 §2.1: easings shipped as `--ease-out` / `--ease-in-out` (no `--motion-ease*`, no ease-in token) — acceptable, but 08's names are now stale documentation. | `styles/tokens.css:85-86` |
| P2 | **Keyframes exist and are token-driven.** `toast-in` (200 ms, `--ease-out`), `skeleton-pulse` 1.4 s, `spin` 0.7 s — vs zero `@keyframes` at 151. | `BaseLayout.astro:138-146` |
| P3 | **Toast region is global.** Fixed bottom-right, `--z-toast`, `aria-live="polite"`, rendered on every page via the layout; entrance is fade + 8 px rise per 08 §2.2. | `BaseLayout.astro:137-140,225` |
| P4 | **Skeleton + spinner CSS exist**; checkout is the model consumer: 3 static skeleton lines under its loading card and a real spinner in the processing area. | `BaseLayout.astro:143-146`; `checkout/[id].astro:47-49,189` |
| P5 | **`window.confirm()` is gone.** Grep finds no `window.confirm`/`window.alert` in pages; revoke moved to native `<dialog>` (but see R1 — the call is broken). | `api-keys.astro:509-518` |
| P6 | **Checkout polling exists (08 P0, partial).** 6 s interval while `processing`, stops on any terminal status, manual *Check status* button, transient errors swallowed so polling continues, completed/expired/failed views all rendered. | `checkout/[id].astro:605-642,745-760` |
| P7 | **Payer field validation is the system pattern, finally.** `fieldError`/`clearFieldError` with `aria-invalid` + danger border (`.input[aria-invalid]`, `BaseLayout.astro:107`), focus to the offending field, clear-on-input — on both payer pages. | `checkout/[id].astro:798-803,836-837`; `pay/[slug].astro:547-555,597-599` |
| P8 | **Reduced-motion foundation intact and extended.** Global 0.01 ms collapse retained; skeleton explicitly `animation: none`; `pay/[slug]` still gates smooth-scroll on `matchMedia`, and `ui.ts scrollToEl` encodes the rule. Spinner is neutralised by the global animation-duration rule (it becomes static, not rotating). | `BaseLayout.astro:54-59`; `pay/[slug].astro:590`; `ui.ts:160-165` |
| P9 | **Double-submit protection still universal.** `disabled = true` before fetch in every page (grep: 2–8 sites per page across all 15 interactive pages), restored on error. | all `pages/**/*.astro` |
| P10 | **Ad-hoc busy labels spread** beyond 151's 4: login `Logging in…`, forgot-password `Sending…`/`Updating…`, invoices `Sending…`, webhooks `Saving…`/`Queuing…`, plugins `Saving…`/`Invoking…`. Direction is right; mechanism is wrong (see G2). | `login.astro:91`; `forgot-password.astro:61,72`; `invoices.astro:611`; `webhooks.astro:519,582`; `admin/plugins.astro:278,372` |

---

## 3. Gaps — where users still get no (or wrong) feedback

### G1 · P0 — Checkout polling never starts after the payer submits
`startPolling()` is called from exactly one place: `render()` when a *loaded* status is
`processing` (`checkout/[id].astro:642`). The submit handler (`:810-826`) only disables
the button, unhides `#submitted-area` and paints a `processing` badge — it never calls
`startPolling()`/`refreshStatus()` and never shows `#processing-area` (spinner). A payer
who submits on this page — the normal flow — still sits on a static success alert until
manual reload, exactly the 151 dead end. Polling only helps a payer who *arrives* (or
reloads) while already processing. Secondary poll defects, same file: no
`document.hidden` pause (dashboard has one, `:675-677`), no backoff, and after
30 × 6 s ≈ 3 min polling stops **silently** (`:619`) — no "still processing, safe to
close" state, though that copy exists unused in `#processing-area` (`:190-196`).

### G2 · P0 — `withBusy` unadopted: in-flight feedback is still disabled-opacity on most actions
`withBusy` (spinner + verb label, auto-restore) has **0 call sites**. The ~40 submit/
action handlers still hand-roll `btn.disabled = true`, and most never change the label:
dashboard create-merchant/create-payment, settings saves, gateways save, api-keys
create, checkout submit, pay submit, transactions filter/refund, admin actions. On a
slow connection these look frozen — 08 §2.2 explicitly banned disabled-opacity as the
*only* in-flight signal. The pages that did add labels (P10) each re-implement restore
logic inline, the duplication `withBusy` was built to delete.

### G3 · P1 — Loading is still a sentence on 11 of 12 list/form pages
`Loading X…` text paragraphs remain the loader in dashboard (`index.astro:25`),
api-keys (`:69`), gateways (`:34`), invoices (`:161`), payment-links (`:144`),
webhooks (`:51`), settings (`:35`), transactions (`:74`), admin (`index.astro:31`),
plugins (`:41`), pay (`[slug].astro:33`). `skeletonAfter` (150 ms show-delay, the
flash-killer from 08 §2.3) has **0 call sites**; only checkout hand-writes skeleton
markup. No page sets `aria-busy` anywhere (**0 occurrences** in `frontend/src`), so
assistive tech gets no busy signal and every content arrival is a full layout swap.
Dashboard merchant-switch and section reloads inherit this: content collapses/re-expands
with no skeleton geometry preserved.

### G4 · P1 — Toast built, feedback geography unchanged
`toast()` has **0 direct imports/call sites**; its only reachability is inside
`copyText` (`ui.ts:104-112`), used by checkout/pay copy buttons. All other success
feedback still uses the 151 scatter: **57 `alert-success`/`alert-error` occurrences**,
page-top or form-adjacent persistent alerts on 13 pages, none auto-dismissing, none
with actions. Settings/gateways "Settings saved." (`settings.astro:130`,
`gateways.astro:131`) still lingers with **no dirty-state clearing** — grep for
`unsaved|dirty` across the frontend: 0 hits (08 §2.4 requirement, unbuilt).
Toast implementation gaps vs its own spec, for when it is adopted: error toasts also
auto-dismiss at 5 s (`ui.ts:53` — 08 said errors persist), no pause-on-hover/focus,
no dismiss button, no exit animation (node is removed mid-frame), unbounded stack
(08 capped at 3).

### G5 · P1 — Dashboard 30 s poll is still invisible and unrecoverable
Poll (`dashboard/index.astro:675-677`) is silent by design and remains so: no
"Updating…" indicator next to `#chart-updated` (`:133,660-661` — only a post-hoc
`Updated HH:MM:SS.` stamp), no row-flash on changed transactions (grep `flash` in
pages: 0), chart SVG is wholesale `replaceChildren` with no cross-fade. Failure path
is worse: a failed poll surfaces per-section static alerts via `loadDashboard`'s
sub-loaders, there is **no backoff** (fixed 30 s forever), no "Update failed —
retrying" quiet state, and **no Retry button anywhere in the frontend** (grep `retry`
in pages finds only webhook-delivery data fields). 08 §2.4 items 1+3: unbuilt.

### G6 · P1 — Pay page still has no waiting state at all
`pay/[slug].astro` has no `setInterval`/poll of any kind. Its submit (`:565-595`)
disables the button, then swaps the whole card for a success panel on the POST
response — fine if the POST is terminal, but any gateway/SMS verification that
completes *after* the response is invisible, and during a slow POST the payer gets
a disabled button with an unchanged label (no spinner, no `withBusy`).

### G7 · P2 — Dialog has no motion and leaks DOM nodes
`.mp-dialog` has styling (radius, shadow, `--color-overlay` backdrop) but **no
entrance/exit animation** — no `toast-in`-equivalent, no backdrop fade
(`BaseLayout.astro:149-150`), despite `--motion-slow` existing for exactly this.
`confirmAction` appends a new `<dialog>` to `document.body` on every call and never
removes it (`ui.ts:117-138`), and renders the confirm button as `btn-danger`
unconditionally — a non-destructive confirm would still be styled destructive.

### G8 · P2 — Remaining token/rule bypasses (the 151 bug class, still open)
- Hardcoded `transition: border-color 0.15s ease, box-shadow 0.15s ease` — the last
  raw-duration transition in the app, plus legacy `--surface/--border/--radius`
  aliases in the same block: `dashboard/gateways.astro:166-168`.
- Raw `scrollIntoView({behavior:'smooth'})` bypassing `scrollToEl` **and**
  reduced-motion: `api-keys.astro:546` (new-key box), `gateways.astro:508`
  (config panel). `scrollToEl` has 0 call sites. `pay/[slug].astro:590` does it
  correctly by hand — three pages, three behaviours.
- Panels still pop: gateway config, refund row, 2FA setup, new-key box all toggle
  `hidden` with no expand transition (151 `.hidden`-toggle counts unchanged in
  spirit: 13–45 toggles per dashboard page). No `:active` press state on `.btn`
  (grep `scale(`: 0), no table-row hover transition, no badge pulse on status
  change, no `ClientRouter`/view transitions (grep: 0 — navigation is still a
  full reload everywhere, incl. dashboard; 08 P2, still open).
- Merchant create still ends in `window.location.reload()` (`dashboard/index.astro:917`)
  — the one remaining reload-as-transition, with scroll/focus loss.

### G9 · P2 — Validation stops at the payer boundary
`fieldError`/`clearFieldError` are adopted **only** by checkout + pay. Every
dashboard/auth form still reports one form-level `.alert-error` sentence with no
per-field text, no `aria-invalid`, no focus-to-field (exception carried from 151:
payment-links validation focus, login TOTP reveal). No blur/live validation
anywhere outside the payer clear-on-input listeners.

---

## 4. Ranked fixes (file:line evidence)

| Rank | Fix | Evidence / acceptance |
|---|---|---|
| **R1 (bug)** | **Fix the broken `confirmAction` call before anything ships on it.** `ui.ts:115` signature is `confirmAction(message: string, confirmLabel?)` and does `p.textContent = message`; the sole adopter passes an **object** `{title, body, confirmLabel, danger}` (`api-keys.astro:510-518`), so the revoke dialog renders `[object Object]`, ignores title/body/danger, and labels the button "Confirm" not "Revoke key". Reconcile one contract (recommend: adopt the object form in `ui.ts`, it's the better API) + add dialog cleanup and non-danger variant (G7). | `lib/ui.ts:114-139` vs `dashboard/api-keys.astro:510-518` |
| **R2 (P0)** | **Start checkout polling from the submit handler** and show `#processing-area`; add `document.hidden` pause, backoff, and a calm terminal "still processing — safe to close" state when the cap is hit instead of silent stop. | `checkout/[id].astro:617-630,810-826` |
| **R3 (P0)** | **Adopt `withBusy` on every submit/action button**, deleting the ad-hoc label code (P10 files) and the disabled-only handlers; start with checkout/pay submit, login, dashboard create-payment/merchant, settings/gateways save. Acceptance: grep `withBusy` ≥ 1 per interactive page; no handler whose only in-flight signal is `disabled`. | `lib/ui.ts:57-74` (0 call sites today) |
| **R4 (P1)** | **Adopt `skeletonAfter` on dashboard + list pages**, retiring `Loading X…` paragraphs; add `aria-busy` toggling on the loading region in the same pass. Keep checkout's static skeleton as the geometry reference. | `lib/ui.ts:79-97` (0 call sites); loader lines in G3 |
| **R5 (P1)** | **Route non-critical success through `toast()`** (payment-links/invoices/webhooks/plugins/transactions create+action success), and first fix toast itself: errors persist until dismissed, cap stack at 3, pause on hover/focus, add close button + exit fade. Keep form errors inline. | `lib/ui.ts:46-54`; 57 alert hits in G4 |
| **R6 (P1)** | **Dashboard poll feedback:** "Updating…" spinner beside `#chart-updated` during poll, one-shot row flash on changed rows, quiet failure state with exponential backoff (30→60→120 s) and a header Retry. Also add Retry buttons to section/page error alerts (pattern reusable from R6). | `dashboard/index.astro:133,472-482,660-677` |
| **R7 (P1)** | **Pay page waiting state:** after POST, if the returned status is non-terminal, poll like checkout (shared helper — extract checkout's poll into `lib/ui.ts` or `lib/api.ts` rather than copying it); busy-spinner on submit meanwhile. | `pay/[slug].astro:565-595` (no poll exists) |
| **R8 (P2)** | **Kill the last bypasses:** gateways card transition → `var(--motion-fast) var(--ease-out)` + token colours; `api-keys:546` and `gateways:508` → `scrollToEl`; merchant-create reload → inline list refresh + toast. | `gateways.astro:166-168,508`; `api-keys.astro:546`; `dashboard/index.astro:917` |
| **R9 (P2)** | **Dialog + panel motion:** dialog/backdrop fade-rise at `--motion-slow` in / `--motion-fast` out; expand transition for gateway-config/refund/2FA/new-key panels with focus-in on open (refund already focuses). Add `.btn:active` press (`--motion-instant`). All must collapse under the existing reduced-motion rule — CSS only, no WAAPI. | `BaseLayout.astro:87,149-150` |
| **R10 (P2)** | **Extend `fieldError` to dashboard/auth forms** (settings, gateways, api-keys create, invoices, login/register server errors), and add dirty-state clearing of "saved" alerts on any subsequent edit in settings/gateways. | G9; `settings.astro:130`; `gateways.astro:131` |

---

## 5. `ui.ts` helper adoption ledger (grep-verified)

| Helper (`lib/ui.ts`) | Call sites | Adopters / non-adopters |
|---|---|---|
| `statusBadge` (:40) | 4 files | ✅ invoices `:277,597`, transactions `:129,410`, checkout `:384,687,823`. ❌ **Re-implemented locally 3×**: `dashboard/index.astro:363` (own JS taxonomy — also classifies `refunded` as *failed*, shared taxonomy says info), `webhooks.astro:451` (own fn, knows `retrying`/`dead_letter` the shared map lacks), `api-keys.astro:445-460` (local var named `statusBadge` for device-presence labels — different domain, rename to avoid collision). Admin pages use neither. |
| `toast` (:46) | **0 direct** | Only reached via `copyText`. No page imports it. |
| `withBusy` (:57) | **0 — fully unadopted** | Every page hand-rolls disabled/label logic instead (G2). |
| `skeletonAfter` (:79) | **0 — fully unadopted** | Checkout hand-writes skeleton markup; all other pages use text loaders (G3). |
| `copyText` (:100) | 2 files | ✅ checkout `:602`, pay `:306,310`. ❌ Re-implemented locally in `payment-links.astro:378` (its 151 original, still not migrated — behaviour differs: no toast, no clipboard-API fallback parity) and raw `navigator.clipboard.writeText` in `api-keys.astro:553` (with its own persistent "Copied" text, third pattern). |
| `confirmAction` (:115) | 1 file, **broken** | ⚠️ `api-keys.astro:510` dynamic import — incompatible object argument (R1). No other destructive action (refund, plugin disable, admin status) uses a dialog at all. |
| `fieldError` / `clearFieldError` (:141/:155) | 2 files | ✅ checkout, pay only. ❌ all dashboard/admin/auth forms (G9). |
| `scrollToEl` (:161) | **0 — fully unadopted** | 2 raw smooth-scroll bypasses + 1 correct hand-roll (G8). |
| `brandAccent` (:186) | 2 files | ✅ checkout `:680`, pay `:476` — full adoption where relevant. The ledger's one clean pass. |
| `luminanceOf` / `contrastRatio` (:168/:178) | 0 external | Internal to `brandAccent` (`contrastRatio` not even used by it — dead export, keep only if a test consumes it). |

**Net:** of 8 UI-facing helpers, 3 are fully unadopted (`withBusy`, `skeletonAfter`,
`scrollToEl`), 1 is effectively unadopted (`toast`), 1 is adopted-but-broken
(`confirmAction`), and the 2 best-adopted (`statusBadge`, `copyText`) each still
have live local duplicates the shared version was written to replace.

---

## 6. Reduced-motion re-check (08 §3 rules)

- ✅ Single-switch rule holds for everything CSS-based; skeleton pulse explicitly off.
- ❌ Rule 2 (JS scrolling through one gated helper) still violated at 2 sites (G8) —
  the exact bug class 08 flagged at 151, migrated but not closed.
- ⚠️ Rule 3: under reduced motion the global rule freezes the spinner mid-rotation
  rather than replacing it with text — acceptable only because `withBusy`'s label
  carries the meaning; today the sole spinner (checkout processing) has the
  "Awaiting confirmation" heading beside it, so meaning survives.
- ❌ Rule 8 testing gate: no reduced-motion walk evidence found in this pass
  (login → poll → save → revoke dialog → checkout) — and the revoke leg of that
  walk would currently surface R1.

*End of Review 180 — Motion/Interaction.*
