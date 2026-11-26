# Review 195 — UX Researcher, Design Review #3 (Waves 189–194 era; journeys re-walked at HEAD)

**Scope / method:** Review only, no source edits. Code-walk of three end-to-end
journeys in the redesigned frontend at a clean, committed tree — unlike Review
#1, nothing reviewed here is uncommitted working-tree code:

- Merchant onboarding: `register.astro` → `dashboard/index.astro` (overview +
  onboarding checklist) → `dashboard/settings.astro` / `dashboard/gateways.astro`
  (plus `dashboard/payment-links.astro` where the checklist hands off to it).
- Payer journey: `pay/[slug].astro` + `checkout/[id].astro`, with
  `lib/api.ts`, `lib/ui.ts`, and the Waves 181–188 log entry as supporting
  evidence.
- Forgot-password: `login.astro` → `forgot-password.astro` against the
  `passwordResetRequest` / `passwordResetConfirm` helpers in `lib/api.ts`.

**State:** HEAD `def77d8` (Waves 189–194, Android); the web redesign under
review landed as `509fef5` (Waves 159–180) and `c11dc4c` (Waves 181–188 +
165 fixes), both CI-logged in `WAVES.md`. Review #2's checkout findings
(`review180-checkout.md`) were spot-re-verified against the current files
rather than re-derived; where its line-level findings still hold verbatim,
this review says so.

**Baselines:** `02-ux-researcher.md` (Wave 151, findings F1.x / F2.x and the
Top-10) and `review165-ux.md` (Review #1, findings N1–N9 and fixes R1–R12).

**Headline verdict:** The payer surface is the big mover since Review #1:
steps, gateway radio cards, copyable references, field-level validation,
checkout polling and a brand-contrast gate are all real and committed —
roughly half of the Wave 151 payer friction is now fixed. Onboarding, by
contrast, has not moved at all since Review #1: every one of its new traps
(N1–N4) survives byte-for-byte, activation is still not gated on gateway
configuration, and the checklist's first step still cannot be completed from
the UI. Forgot-password is unchanged — still the cleanest flow in the
product, still carrying the same five small frictions Review #1 listed.
Net Top-10 movement since Review #1: #5 and the validation half of #6/#7
improved; #1 is now proven backend-blocked rather than frontend-neglected;
#2, #3, #4 did not move.

---

## 1. Wave 151 friction points now FIXED (with evidence)

### Payer journey — the bulk of the progress

| 151 point | Status at 195 | Evidence |
|---|---|---|
| F2.3 — No live status anywhere for payers | **FIXED on checkout; open on `/pay` (see §2, R4)** | `checkout/[id].astro:605–642`: `startPolling()` re-checks `checkoutDetails` every 6 s, max 30 ticks (~3 min), stops on any non-`processing` status, tolerates transient errors; manual **Check status** button in the processing area (`checkout/[id].astro:188–198`, handler wired to `refreshStatus()`). Spinner added to the processing panel. |
| F2.2 (part) — No copy affordance for the reference the payer is told to "use exactly as shown" | **FIXED** | Checkout: reference is a `.copy-field` with a Copy button (`checkout/[id].astro:89–95`, handler at `:601–603`) using shared `copyText()` in `lib/ui.ts`. `/pay` success: both Transaction ID and Reference are copy fields with copy buttons (`pay/[slug].astro:131–136`, handlers `:304–311`). |
| F2.2 (part) — Gateway `<select>` UX | **FIXED as a control; not as a data problem** | `pay/[slug].astro:101–107`: the `<select>` is gone, replaced by six full-label gateway radio cards (large tap targets, checked state visible via border/background/ring). The list is still hard-coded — that half is in §2, R4. |
| F2.6 — No field validation before submit on the highest-stakes form | **FIXED, with one undermining fallback (see §2, R9)** | `checkout/[id].astro:792–807`: sender number checked against the BD pattern `^(\+?880\|0)1[3-9]\d{8}$`, TrxID against 6–20 alphanumerics, errors attached to the field via `fieldError()` (`lib/ui.ts`) and cleared on input (`checkout:828–829`). `pay/[slug].astro:543–558`: same field-level treatment for collected phone/email. |
| F2.5 (part) — Thin trust signals | **IMPROVED / fixed as designed** | Identical lock-icon trust row on both payer pages (`checkout/[id].astro:215–219`, `pay/[slug].astro:119–122`), including the "Never share your wallet PIN" line the pay page lacked in 151. Merchant brand colour now passes a contrast gate before theming the surface: `brandAccent()` in `lib/ui.ts` requires ≥3:1 white-on-brand or drops to the platform default, applied on both pages (Waves 181–188 log, `WAVES.md`). |
| (151 missing-state) — Payer pages had no progress/step structure | **FIXED** | Identical 3-step indicator (Review → Pay → Confirmed, EN+BN) on both pages (`checkout/[id].astro:38–42`, `pay/[slug].astro:27–31`), `aria-current` managed in JS; checkout also gained skeleton loading lines. |

### Auth / onboarding surface — fixed in 159–168, confirmed still fixed at 195

| 151 point | Status at 195 | Evidence |
|---|---|---|
| F1.5 — No "Forgot password" link; permanent-lockout trap | **FIXED** | `login.astro` password field carries `<a href="/forgot-password">Forgot your password?</a>`; `forgot-password.astro` exists on the shared auth shell (see §3). |
| F1.5 — Three brand primaries in the first three screens (teal/blue/violet) | **FIXED** | One token-driven `.btn` primary across landing, login, register and forgot-password; all four auth pages use the same `.auth-wrap` / `.auth-card` / `.auth-brand` shell (`register.astro:8–11`, `login.astro:8–11`, `forgot-password.astro:8–11`). |
| F1.3 (part) — Onboarding checklist items were display-only badges | **PARTIALLY FIXED — links exist; two are dead ends (§2, R3/R6)** | `dashboard/index.astro:519–541` (`STEP_LINKS`, Wave 164): unfinished steps render as `<a href>label →</a>` instead of plain text. |
| F3.6 (quick win) — Admin link shown to every merchant | **FIXED for Admin; re-introduced for Plugins (§2, R7)** | `components/DashboardNav.astro`: Admin is `adminOnly: true`, hidden until `me().is_admin` resolves. |
| Review #1 N9 — Reviewed work existed only in an uncommitted tree | **FIXED** | All of 159–188 is committed (`509fef5`, `c11dc4c`) with `WAVES.md` entries and logged CI verdicts; working tree is clean at `def77d8`. Review #1's R1 is discharged. |

### Forgot-password — fixed at Review #1, re-verified unchanged at 195

The flow still matches its API exactly, end to end: `passwordResetRequest`
→ `POST /api/v1/auth/password-reset/request` and `passwordResetConfirm` →
`POST …/confirm` (`lib/api.ts:933–947`); the page's copy is honest against
the backend on all three points Review #1 checked — "valid for 1 hour"
(`forgot-password.astro:13`), the anti-enumeration success text "If that
account exists, a reset token is on its way" (`:24–26`), and "Resetting ends
all existing sessions" in the password hint (`:41`). No regression found.

---

## 2. Remaining friction — ranked

### R1 (P0, blocker, backend-owned) — Checkout still never shows the number to send money to (151 #1 / F2.1 — NOT fixed, now proven not frontend-fixable)

`checkout/[id].astro` still renders only the gateway *name*
(`gatewayDisplay()`), and the default instruction text at `:127–131` still
says, in effect, "send the amount shown above to the merchant using the
payment method above" — with no receiving wallet number anywhere on either
payer page. The Waves 181–188 log states the constraint honestly
(`WAVES.md`): "the public checkout API does not expose the merchant's
receiving wallet number or per-gateway instructions, so the copyable
recipient-number field can only render when a future API change exposes
it — flagged for a backend decision, out of design-program scope."
Review #2 (`review180-checkout.md` §3) verified the same against
`checkout_details` and `PublicLinkView`. Credit: the design does not fake a
number, and the copy-field infrastructure is built and waiting. Debit: no
line on either page tells the payer the number must come from the merchant
out-of-band, and no merchant-facing copy tells merchants the checkout
message field is currently the only place that number can appear. This is
still the single largest drop-off in the product; it is just no longer a
frontend finding. **It needs a named backend/API decision before v0.0.3,
not another design wave.**

### R2 (P0, drop-off) — Activation is still not gated on gateway configuration (151 #2 / F1.1 — unchanged since Review #1)

Nothing in the activation path warns a merchant who has no enabled gateway:

- Register's lede still promises only "create your merchant from the
  dashboard to start accepting payments" (`register.astro:13–16`) — the
  gateway requirement is still unmentioned at signup.
- Dashboard Create-payment still populates its gateway `<select>` from the
  **public catalogue**: `dashboard/index.astro:391` calls `listGateways()`;
  `merchantGateways()` (which exists, `lib/api.ts:832`, and is used
  correctly by `dashboard/gateways.astro`) is still never called on the
  overview page.
- Payment-link creation (`dashboard/payment-links.astro`) still performs no
  gateway-configuration check.
- The gateways page itself is good — it names the merchant
  (`gateways.astro:28`), explains the merchant number field (`:63–69`), and
  states configured/not-configured per gateway (`:459–460`) — which makes
  the seam worse, not better: the one page that knows the state is never
  pushed in front of a merchant who hasn't visited it.

Review #1's R4 acceptance still applies verbatim: warn inline on
Create-payment and Create-link when the merchant has no enabled gateway,
and populate Create-payment from `merchantGateways()`.

### R3 (P0, redesign-introduced, still open) — The checklist's first step is still uncompletable (Review #1 N1)

Unchanged in every particular:

- `dashboard/index.astro:520`: `email_verified: '/dashboard/settings'`.
- `dashboard/settings.astro` still contains exactly three sections —
  Branding (`:48`), Notifications (`:83`), Payouts (`:97`) — and no
  email-verification UI.
- `lib/api.ts` still has **no** email-verification functions (grep for
  `emailVerif` / `email-verif`: zero matches; the only auth-flow helpers are
  the password-reset pair at `:933–947`).

A fresh merchant's checklist still caps below 100%, the card can never
hide, and the first link a new user is likeliest to click still leads to a
page with nothing to do on it. This remains the worst *pure-frontend*
onboarding defect because the Wave 89 backend endpoints already exist —
it is a missing client + one Settings section, nothing more.

### R4 (P1) — `/pay` still ends at a false summit, and still offers gateways the merchant may not have (151 #7 / F2.2 — half-fixed)

The redesign improved the success panel's furniture (copyable Transaction
ID + Reference, steps advance, reduced-motion-aware scroll —
`pay/[slug].astro:571–586`) but not its contract:

- Success is still headed "Payment started" (`pay/[slug].astro:125`) and
  the flow still collects **no sender number and no TrxID**, so if SMS
  matching fails there is no in-page recovery — the exact gap checkout's
  form exists to close. The two payer journeys still ask for different
  things in a different order (151's core F2 complaint).
- `/pay` has **no polling at all** — no `setInterval` / `startPolling` in
  the file; the payer who stays sees a static "started" panel forever, and
  the payer who leaves has no status view to return to.
- The gateway cards are still the hard-coded six (`pay/[slug].astro:102–107`)
  with `bkash` pre-`checked`, not the merchant's enabled gateways — a payer
  can still "pay" via a gateway the merchant never configured. (Partly
  API-bound: `PublicLinkView` returns no gateway list — but pre-selecting
  bKash is a frontend choice.)
- The displayed "reference" still falls back to the public slug
  (`pay/[slug].astro:571`: `result.reference ?? slug`), presented in a
  ceremonial copy field with no explanation of where it is used.

### R5 (P1, defect inside a fix) — Checkout polling starts one step too late; checkout still has no gateway choice (Review #2 R-04 / C-03)

Both Review #2 findings re-verified verbatim at HEAD:

- Polling only starts inside `render()` when a *loaded* status is already
  `processing` (`checkout/[id].astro:641–642`). The primary path — payer
  submits the form on this page — hides the pending area and shows the
  static `#submitted-area` (`checkout/[id].astro:819–825`) **without**
  calling `startPolling()` or `refreshStatus()`. The payer who most needs
  live confirmation must reload to get it. Polling also stops silently
  after ~3 minutes with no escalation copy.
- The steps indicator mislabels that wait: `render()` maps `processing`
  to step 3, which is labelled "Confirmed" (`checkout/[id].astro:641`) —
  the indicator asserts the one thing the page copy correctly refuses to.
- Checkout's gateway is still a read-only summary row; submission silently
  pins `details.gateways?.[0]` / the single `gateway`
  (`checkout/[id].astro:705–708`, submitted at `:817`). A Nagad-only payer
  handed a bKash-first checkout can only abandon. (`/pay` got radio cards;
  checkout did not.)

### R6 (P1) — The rest of the checklist is still a map with wrong turns (Review #1 N2–N4 + F1.3 silent-hide)

All present verbatim in `dashboard/index.astro:519–528`:

- `kyc_submitted: '/dashboard'` — a self-link to the top of the page the
  user is already on, while the KYC card sits below Search, Chart and
  Create payment. An anchor target now exists (`id="kyc-heading"`,
  `dashboard/index.astro:228–229`) and the link still doesn't use it.
- `first_payment: '/dashboard/payment-links'` — identical to
  `payment_link_created`; receiving a first payment is not creating a link.
- `webhook_configured` is still in `STEP_LINKS` although the backend
  onboarding handler (`crates/api/src/handlers/product.rs:77–82`) emits
  seven steps and `webhook_configured` is not one of them — front/back
  drift of exactly the kind the token lint catches elsewhere.
- On any fetch error the card still hides itself
  (`catch { card.hidden = true }`, `dashboard/index.astro:551–553`), so a
  broken checklist is still indistinguishable from a completed one
  (Review #1 R12, unaddressed).

### R7 (P1) — Merchant context is still invisible, and the shell still contains a trap link (151 #10 / F1.6; Review #1 N7/N8)

- `components/DashboardNav.astro` spends its context slot on the **user**
  (account email, language, logout) — there is still no active-merchant
  name or switcher in the shell. The switcher still exists only on
  dashboard home and settings; transactions / payment-links / gateways
  still silently resolve "stored `mp_merchant` id or first merchant"
  (e.g. `gateways.astro:342–355, 541–555`). A two-merchant user can still
  create a link or refund under the wrong merchant with no on-page signal
  beyond payment-links' small "Merchant: …" line.
- The Plugins nav item (`DashboardNav.astro` links list) is still **not**
  `adminOnly`, while pointing at `/admin/plugins` — non-admin merchants
  still get a Configure-group link whose only outcome is an
  "Admin access required" card. Review #1's R6 was a one-mechanism fix
  (the `adminOnly` mechanism is in the same file) and it was not taken.

### R8 (P2) — Forgot-password: the same five frictions, untouched (Review #1 N5)

The flow works and is honest (§1), but `forgot-password.astro` at HEAD is
identical in behaviour to Review #1:

1. **No `?token=` prefill** — no `URLSearchParams` / location parsing in the
   file; if the email ever becomes a link, the page cannot consume it. The
   token must be hand-pasted, with no paste help.
2. **No confirm-password field** on the confirm form (`:35–45`) — a typo in
   the new password is discoverable only at the next login.
3. **No client-side validation** — neither form checks empty fields, email
   shape, or the ≥8-chars-with-a-digit rule before submit; the rule is
   learned only from the server error banner. (Login gained an empty-field
   check with focus move, `login.astro:82–87`; its sibling forms did not —
   the inconsistency Review #1 flagged in register persists here too.)
4. **Both forms stay live after success** — the confirm handler shows
   `#confirm-ok` and re-enables its button (`:70–76`); a spent token can be
   re-submitted into a raw server error, and success is a link the user
   must notice ("Password updated — sign in…", `:43–45`) rather than a
   redirect to `/login` with a flash.
5. **The friendly fallback is still unreachable** — the confirm handler's
   "it may have expired" text (`:74`) only fires for a non-`Error` throw;
   `apiFetch` always throws `Error`, so users see the raw server message.

Plus the cross-auth item from Review #1 N6, still open: `.auth-brand` is
still a `<p>`, not a link, on all three auth pages
(`register.astro:11`, `login.astro:11`, `forgot-password.astro:11`) — auth
pages remain a one-way street with no route back to the marketing page.

### R9 (P2) — Auth forms generally: register seam, proactive 2FA, validation (151 F1.2 / F1.4; 151 #8 remainder)

- **Register is byte-equivalent in behaviour to 151.** Still `register()`
  then a second `login()` round-trip (`register.astro:93–94`) with the same
  stranded state — "Registration succeeded but sign-in returned no token.
  Please sign in." (`:101`) — leaving an existing account's owner on the
  register form. Still `novalidate` with **no** client-side checks at all
  (not even empty fields), no show-password, no strength feedback beyond
  the prose hint (`:32`), no terms/privacy acceptance, and errors only in
  the single top-of-form `role="alert"` (`:36`).
- **2FA is explained better but still not proactive.** `login.astro:30–35`
  now carries the hint "This account has two-factor authentication
  enabled", but the field is still `hidden` until the server returns
  `totp_required` / `totp_invalid` and the page string-matches it
  (`login.astro:113–125`). A user who knows they have 2FA must still fail
  once to see the field. No "Use a two-factor code" reveal exists.
- Merchant creation success is still `window.location.reload()`
  (`dashboard/index.astro`, create-form handler) with no next-step CTA —
  the new merchant is not routed to gateways, the step R2 shows they most
  need.

### R10 (P2) — Failure recovery, expiry, and receipts on payer pages (151 #6 / F2.4 — largely unchanged)

- Checkout's terminal `#status-area` still says only "This checkout is no
  longer awaiting payment… please contact the merchant"
  (`checkout/[id].astro:755–762`) — no retry, no new-payment action, and no
  merchant contact detail is rendered on either payer page, so "contact
  the merchant" names an action the page gives no means to perform.
- `/pay` load failure still collapses inactive / expired / exhausted /
  unknown into one card (`pay/[slug].astro:35–43`) whose only advice is
  "ask the merchant for a fresh link"; the raw API message is surfaced
  verbatim (`:524–527`). Checkout load errors likewise surface raw API
  text (`checkout/[id].astro:771–775`). No Retry button on either page.
- Expiry is still a locale date string, never a countdown
  (`checkout/[id].astro:716–722`), with no mid-payment lapse handling.
- Neither payer page persists in-progress input (no
  `sessionStorage`/`localStorage` use in either file): Android discarding
  the tab while the payer is in the bKash app still loses a typed TrxID.
- Completed is still not a receipt: `#completed-area`
  (`checkout/[id].astro:201–206`) shows no amount, gateway, TrxID,
  timestamp, or return-to-merchant action — no proof artefact for the
  Messenger dispute through which these payments are actually settled.
- The FX "display currency" selector still sits on both payer pages
  (`checkout/[id].astro:107–111`, `pay/[slug].astro:53–57`) calling the
  auth-required `/api/v1/fx/rates` (`pay/[slug].astro:373`) — still dead
  weight for the anonymous payers it targets (151 F2.5, quick win, not
  taken).

### R11 (P2) — Payment-link handover and lifecycle (151 F1.7 / F1.8 — unchanged)

On create, `payment-links.astro:572` still shows only
`Payment link "…" created.` and reloads the list; the merchant must find
the new row and press its Copy button (the per-row copy itself works,
`:442–451`). There is still no success panel containing the URL, and no
share action — while the page's own lede and the landing page sell
"share it anywhere". The header comment at `payment-links.astro:15–18`
still states the API exposes no deactivate/delete, so a mistyped-amount
live link still cannot be switched off. Both halves are now explicitly
API/product decisions that have been pending since Wave 151, not design
oversights.

### R12 (P3, carried, out of the three walked journeys but confirmed) — Disputes and Payouts UI still do not exist (151 #3 / #4)

Confirmed by listing, not re-walked: `frontend/src/pages/dashboard/`
still contains the same eight pages — no disputes, no payouts/settlements
page; "dispute" in the dashboard still appears only as webhook event names
(`webhooks.astro:125–126`). Settings' Payouts section is still
schedule/day/threshold only (`settings.astro:97–128`), and the overview
balance card still reads "Available balance — Ready to withdraw"
(`dashboard/index.astro:74–76`) with no action attached. Review #1's R8
asked for these to be scheduled or explicitly deferred; neither happened,
so they will reach v0.0.3 absent by default rather than by decision.

---

## 3. Journey verdicts in one line each

- **Onboarding (register → checklist → settings/gateways):** the shell and
  the gateways page are genuinely better; the *sequence* is not — a new
  merchant can still finish every completable step believing they are live
  while no gateway is enabled (R2), and cannot finish the checklist at all
  because its first step is a dead end (R3).
- **Payer (pay / checkout):** the best-improved journey — validation,
  copying, steps, trust and checkout polling are real fixes — but it still
  cannot tell the payer where to send money (R1, backend-owned), `/pay`
  still stops before the payment does (R4), and the polling fix misses the
  submit path most payers actually take (R5).
- **Forgot-password:** still the model flow for API honesty; its remaining
  friction is five small, cheap, already-diagnosed items (R8) that have
  now survived two review cycles untouched.

## 4. Suggested order for the remaining waves (195–200)

1. **Backend decision + API change for the receiving number** (R1) — the
   only P0 the design program cannot close itself; everything else on this
   list is smaller than this one's drop-off.
2. **Email-verification client + Settings section** (R3) and **checklist
   link repairs** — KYC anchor, `first_payment` target, phantom
   `webhook_configured`, error-state instead of silent hide (R6). One small
   wave makes the checklist completable and truthful.
3. **Activation gating** (R2): `merchantGateways()` in Create-payment +
   inline no-gateway warnings on Create-payment / Create-link.
4. **Submit-path polling + steps-label fix** (R5) and **`/pay` status
   check** (R4, frontend half): start polling from the submit handler;
   give `/pay` success a Check-status affordance.
5. **Shell merchant context + Plugins admin-gating** (R7) — both are
   one-surface changes in `DashboardNav.astro`.
6. **Forgot-password tidy** (R8): confirm-password, client-side rule
   check, disable-on-success + redirect-with-flash, `?token=` prefill,
   `.auth-brand` as a home link on all auth pages.
7. **Written decision on Disputes/Payouts and link deactivation** (R11,
   R12) before the v0.0.3 release notes claim the redesign is complete.

---

*Review only — no source files were modified. All findings are grounded in
files read at HEAD `def77d8` on 2026-11-26; payer line references were
cross-checked against `review180-checkout.md` and re-verified where cited.*
