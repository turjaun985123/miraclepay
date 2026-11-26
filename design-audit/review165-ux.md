# Review 165 #1 — UX Researcher, Design Review #1 (Waves 159–168 re-walk)

**Scope / method:** Code-walk only, no live backend, no interviews — same method as
`02-ux-researcher.md` (Wave 151). Re-walked: `index.astro` (landing),
`register.astro`, `login.astro`, `forgot-password.astro` (new),
`dashboard/index.astro` (onboarding checklist), `lib/api.ts`, plus the backend
contracts they depend on (`crates/api/src/handlers/product.rs` onboarding,
`crates/api/src/handlers/auth_flows.rs` + `routes.rs` password reset) and
`components/DashboardNav.astro` where it changes journey context.

**State caveat — read first:** Waves 152–158 are committed + CI-verified
(`db88433`, run 45807726561). Everything reviewed for 159–168 is **uncommitted
working-tree code** (`git status`: landing/login/register/dashboard/nav modified,
`forgot-password.astro` untracked, no WAVES.md log entry past 152–158, no CI
verdict). Verdicts below are "true of the tree", not "shipped". Landing it as one
reviewed commit is fix R1 below.

**Headline verdict:** The seams I flagged in 151 are closing on the auth surface —
one primary colour, one auth shell, a real forgot-password flow that matches its
API exactly, and checklist items that are finally links. But the redesign has
created one new trap worse than the original dead end: the checklist's first step
now *links* to a page where the action does not exist, so onboarding can never
reach 100% from the UI. Activation gating (my #2) is still only cosmetic, and
the two missing jobs (disputes, payouts) remain missing and are not in the
169–180 dashboard roadmap either.

---

## 1. Journey re-walks

### 1a. Register → onboarding

Steps are unchanged in count (landing → register → create merchant → gateway →
KYC → first link), but three steps changed quality:

- **Landing → register** is cleaner: one token-green primary everywhere
  (`.btn` on landing, login, register — the teal/blue/violet split of F1.5 is
  gone), wordmark + logo mark, eyebrow corrected to v0.0.2, ৳0/0% in the hero
  note instead of buried in pricing prose.
- **Register itself barely moved.** Still `register()` then a second `login()`
  round-trip with the same stranded state ("Registration succeeded but sign-in
  returned no token. Please sign in."), still `novalidate` with **no** empty-field
  check at all — login gained a client-side empty check in its redesign,
  register did not, so the two sibling forms now behave inconsistently. No
  show-password, no strength feedback beyond the prose hint, no terms/privacy.
  The lede still promises only "create your merchant from the dashboard" — the
  gateway requirement (F1.1) is still not mentioned at the point of signup.
- **Onboarding checklist (Wave 164) — verified in `dashboard/index.astro`.**
  `loadOnboarding()` now builds a `STEP_LINKS` map and renders unfinished steps
  as `<a href>label →</a>`. That part of the claim is true. The details are not
  all true — see §3, N1–N4. Also unchanged: the card still hides itself silently
  on any fetch error (`catch { card.hidden = true }`), still sits below the
  balance cards, and dashboard Create-payment **still** populates its gateway
  `<select>` from the public catalogue `listGateways()` — `merchantGateways()`
  is still never called on this page — and payment-link creation still never
  warns when no gateway is enabled. The checklist is now a linked list, not an
  activation gate.

### 1b. Login + forgot-password

- **Login (161–162):** shared `.auth-wrap`/`.auth-card` shell, forgot link
  present under the password field (`<a href="/forgot-password">`), one primary,
  empty-field validation with focus move, busy state on submit. Still open:
  the 2FA field is still hidden until the server returns `totp_required` /
  `totp_invalid` and the page string-matches the error — a user who knows they
  have 2FA must still fail once (Wave 166 is scoped "auth states (2FA/forgot
  UX)", so this is owned, just not done).
- **Forgot-password (new page) — flow verified against the API, end to end:**
  - `lib/api.ts`: `passwordResetRequest(email)` → `POST
    /api/v1/auth/password-reset/request`, `auth: false`, body `{email}`;
    `passwordResetConfirm(token, newPassword)` → `POST
    /api/v1/auth/password-reset/confirm`, body `{token, new_password}`.
  - Backend (`auth_flows.rs`, `routes.rs`): both routes exist with exactly
    those paths/bodies (`EmailReq{email}`, `ResetConfirmReq{token,
    new_password}`); request always returns `200 {ok:true}` whether or not the
    account exists, issues a 1-hour token and queues it by email; confirm
    validates ≥8 chars + a digit, spends the token, and revokes every refresh
    token. The page's copy is honest against all of this: "valid for 1 hour",
    the anti-enumeration success text ("If that account exists, a reset token
    is on its way"), and "Resetting ends all existing sessions" all match the
    implementation. **This is the cleanest new flow in the redesign.**
  - Friction inside it: §3, N5.

### 1c. Landing → register conversion path

Path: nav CTA / hero "Get started free" / pricing CTA → `/register` (3 entry
points, all one primary, all consistent — good). Hero now leads with the SMS
verification story + a verify-card visual instead of the repeated sentence of
151, gateways are grouped by region, pricing is a single ৳0 card. Remaining
conversion friction:

- **No way back:** the auth shell's brand line (`.auth-brand`) is a `<p>`, not a
  link, on login/register/forgot — a visitor who lands on register from the hero
  cannot return to the marketing page except via the browser back button.
- **Expectation gap is narrowed, not closed:** the hero note now says "You run
  it on your own server" (credit — that disclosure exists), but the CTA still
  reads "Get started free" with no hint that "getting started" presupposes a
  running instance; GitHub / API-docs links live only in the footer.
- **No persistent CTA:** after the hero, the next register CTA is the pricing
  card at the very bottom; features/how sections in between have none.
- Mobile nav wraps links to a second, horizontally scrolling row (order:3,
  full width) — workable, but Features/Gateways/Pricing/Login compete with the
  CTA for the first screen.

---

## 2. Wave 151 Top-10 — status at Review #1

| # | 151 fix | Status | Evidence |
|---|-----|--------|----------|
| 1 | Show receiving number on checkout by default | **OPEN — scheduled 181–188 (Wave 182)** | Confirmed: the only working-tree change to `checkout/[id].astro` / `pay/[slug].astro` is token colour (`var(--color-brand)`); no recipient number, no gateway selection, no polling. Correctly untouched — checkout waves have not started. |
| 2 | Gate activation on gateway configuration | **PARTIAL** | Checklist steps are links (164, §1a). Not done: no warning/gate on payment or link creation, Create-payment still lists the public catalogue (`listGateways()`), not configured gateways; checklist still silently hides on error; two of its links are dead ends (N1/N2). |
| 3 | Build the Disputes UI | **STILL OPEN — unscheduled** | No disputes page in `dashboard/` (8 pages, none disputes), no dispute type/function in `api.ts`; "dispute" still appears only as webhook event names. Backend has existed since Wave 38. Not in the 169–180 roadmap either — it will still be open at Review #2 unless re-scoped. |
| 4 | Build the Payouts UI | **STILL OPEN — unscheduled** | `settings.astro` is still schedule-only (schedule/day/threshold); balance card still says "Ready to withdraw" with no action; no settlements/payout history page, no withdraw, despite the settlements API (Wave 18). Also absent from 169–180 as scoped. |
| 5 | Live payer status / polling | **OPEN — scheduled 181–188 (Waves 183/185)** | Confirmed unchanged; no `setInterval` added to either payer page. |
| 6 | Failure recovery on payer pages | **OPEN — scheduled 181–188** | Confirmed unchanged; `#status-area` copy untouched. |
| 7 | Unify the two payer journeys | **OPEN — scheduled 181–188 (Wave 184)** | Confirmed unchanged; `/pay` still hard-codes its gateway list and collects no TrxID. |
| 8 | Fix auth dead-ends | **MOSTLY FIXED** | Forgot-password page exists, is linked from login, and matches the API exactly (§1b); one brand primary across landing/login/register via tokens; shared auth shell. Remaining, owned by Wave 166: proactive 2FA field, show-password, strength feedback, register-side client validation, terms/privacy. |
| 9 | Transactions as a work surface | **PARTIAL — early 171 work in tree** | Working tree adds pagination (25/page, prev/next), shared `statusBadge()` from `lib/ui.ts`, right-aligned money. Still open: date-range + text search in the UI (API params exist), gateway as a select, CSV export (backend Wave 105), row detail, distinct first-run vs filtered empty states, Retry on load error. |
| 10 | Visible merchant context + link lifecycle | **PARTIAL** | New shell (DashboardNav, in tree) shows account email + logout on every dashboard page and hides Admin for non-admins (a 151 quick win — done). But merchant context did not move: switcher still exists only on dashboard home + settings; the shell shows the *user*, not the active *merchant*, so wrong-merchant risk (F1.6) is unchanged. Payment-links: still no success panel with URL/share, still no deactivate (page header comment still states the API offers none). Due in 169/172. |

**Score: 0 fully fixed, 1 mostly fixed (#8), 3 partial (#2, #9, #10), 2 open
and unscheduled (#3, #4), 4 open and correctly scheduled in 181–188 (#1, #5,
#6, #7 — status confirmed, no early or duplicate work found in the tree).**

---

## 3. New friction introduced by the redesign

- **N1 — The checklist's first step is uncompletable (P0, new).**
  `STEP_LINKS.email_verified → /dashboard/settings`, but Settings contains no
  email-verification UI at all (sections: Branding, Notifications, Payouts),
  and `api.ts` has **no** email-verification functions anywhere in
  `frontend/src` — the Wave 89 endpoints (`POST
  /auth/email-verification/request|confirm`) have no client. Registration sets
  `email_verified: false` (`crates/api/src/handlers/auth.rs`). Result: a real
  merchant's checklist caps at 6/7 (86%), the card can never hide, and the one
  step a new user is most likely to click first leads to a page with nothing to
  do. A linked dead end is worse than the 151 badge-only dead end: it promises
  an action and breaks the promise.
- **N2 — KYC link is a self-link with no anchor.** `kyc_submitted →
  /dashboard` — the page the user is already on, landing at the top. The KYC
  card still sits below Search, Chart, Create payment and Recent transactions;
  the link saves zero scrolling. It needs `#kyc-heading` (or the card's id) and
  ideally a highlight/scroll — the pattern exists (`lib/ui.ts` reduced-motion
  scroll helper).
- **N3 — Two different steps link to the same page, one of them wrongly.**
  `first_payment → /dashboard/payment-links`, identical to
  `payment_link_created`. Receiving a first payment is not creating a link;
  sending a user who has a link but no payment back to the link list teaches
  them nothing. Better target: dashboard Create-payment (anchor) or the pay
  page of their existing link.
- **N4 — Phantom step mapping.** `STEP_LINKS` includes `webhook_configured`,
  which the backend onboarding handler never emits (its 7 steps are
  `email_verified, gateway_configured, kyc_submitted, payment_link_created,
  api_key_created, branding_set, first_payment` — `product.rs`). Harmless
  today, but it is exactly the front/back drift the token lint was built to
  catch elsewhere; either the backend gains the step or the map loses the key.
- **N5 — Forgot-password asks for a paste, and gives the paste no help.**
  The emailed artefact is a raw token (backend email body is token-only, no
  link), so the two-form one-page design is forced — but the page adds no
  support for it: no `?token=` prefill if the email ever becomes a link, no
  confirm-password field, no client-side length/digit check before submit
  (the server rule is only discovered from the error banner), both forms stay
  live after success (a spent token can be re-submitted into a raw error), and
  the success state is a link the user must notice rather than a redirect with
  a "password updated" flash on login. Also: the confirm handler's friendly
  fallback message is unreachable in practice — `apiFetch` always throws
  `Error`, so users see the raw server message, not "it may have expired".
- **N6 — Auth pages are a one-way street.** `.auth-brand` is not a link on any
  of login/register/forgot (§1c). The redesigned shell removed the old pages'
  surrounding context without replacing the home affordance.
- **N7 — The new shell repeats the old Admin mistake for Plugins.**
  Working-tree `DashboardNav.astro` correctly hides `Admin` behind
  `me().is_admin`, but `Plugins` sits in the Configure group, **not**
  admin-gated, while pointing at `/admin/plugins` — a page that swaps in an
  "Admin access required" card on 403. Non-admin merchants get a nav item
  whose only outcome is a refusal: F3.6 re-introduced for one link.
- **N8 — Shell context is account, not merchant (missed correction).** The new
  sticky shell spends its context slot on user email + language + logout. All
  valuable — but the 151 finding was *merchant* invisibility, and a
  two-merchant user still cannot tell which merchant Transactions or
  Payment Links is acting on. The redesign had the right surface in hand and
  answered the adjacent question.
- **N9 — Uncommitted-review risk.** 159–168 exists only in a working tree that
  also contains an unrelated transactions change (171 started early) and a
  token-lint edit. Until it lands as CI-verified commits with WAVES.md entries,
  every "fixed" above is provisional — Review #2 should not have to re-derive
  which tree it is reviewing.

---

## 4. Ranked fixes for Waves 166–180

| Rank | Fix | Wave slot | Why / acceptance |
|------|-----|-----------|------------------|
| R1 | **Land 159–168 as reviewed, CI-verified commits + WAVES.md log** (split the early-171 transactions work into its own wave) | before 166 | Nothing else in this review is durable until this happens (N9). |
| R2 | **Make `email_verified` completable or remove the step**: add the verification request/confirm UI (Settings section or a banner on dashboard home) using the existing Wave 89 endpoints + `api.ts` functions; until then, drop the link and render the step as informational | 166 (auth states) | N1 — the only P0 the redesign created. Acceptance: a fresh merchant can reach 100% and the card hides. |
| R3 | **Anchor the checklist links**: KYC → dashboard `#kyc` anchor with scroll/highlight; `first_payment` → Create-payment anchor or the merchant's live pay link; delete or backend the phantom `webhook_configured` key | 166–168 (small) | N2–N4. Acceptance: every unfinished step lands the user on the control that completes it. |
| R4 | **Finish activation gating (rest of #2)**: warn inline on Create-payment and Create-link when the merchant has no *enabled* gateway; populate Create-payment from `merchantGateways()`, not the public catalogue | 170/172 (dashboard) | The highest drop-off risk from 151 is still live; the checklist alone does not close it. |
| R5 | **Merchant in the shell (rest of #10)**: move the merchant switcher (or at minimum the active-merchant name) into `DashboardNav`'s shell bar so every dashboard page carries it | 169 (dashboard shell) | N8/F1.6 — 169 is building this surface anyway; doing it later means touching every page twice. |
| R6 | **Gate the Plugins nav item on `is_admin`** alongside Admin | 169 | N7 — one-line class of fix, same mechanism already in the file. |
| R7 | **Complete Wave 166's auth-states scope**: proactive 2FA reveal ("Use a two-factor code" toggle), show-password on login/register/forgot, password-strength feedback + register-side empty/format validation matching login's, confirm-password + client-side rule check on forgot-confirm, disable both forgot forms after success, make `.auth-brand` a home link on all auth pages | 166 | Rest of #8 + N5/N6. Acceptance: no auth action requires a failed submit to discover. |
| R8 | **Schedule Disputes + Payouts UI into 169–180 or say so explicitly**: at minimum a Disputes list/detail (backend Wave 38, `api.ts` client + one page) and a Payouts page (history + next payout + withdraw entry point over the Wave 18 settlements API) | propose 175a/176a, or a written deferral to 181+ | #3/#4 — currently they fall between the dashboard block and the checkout block and will reach v0.0.3 absent by default, not by decision. |
| R9 | **Finish Transactions (rest of #9)**: date-range + TrxID/reference search, gateway select of configured gateways, CSV export button, distinct empty states, Retry on error, clickable row → detail | 171/178 | Backends already exist (Waves 105/106); this is pure UI completion of a wave already started in the tree. |
| R10 | **Payment-link handover (rest of #10)**: success panel with the URL + copy + native share; surface the still-missing deactivate as an explicit API-gap decision (add the endpoint or stop promising lifecycle in copy) | 172 | F1.7/F1.8 unchanged since 151; the landing page still sells the sharing story the dashboard doesn't deliver. |
| R11 | **Landing conversion tidy**: sticky slim CTA after the hero on scroll, one mid-page CTA (after How-it-works), "self-hosted — bring your own server" qualifier next to the hero CTA, Bangla pass must not regress the ৳0/0% prominence | 167/168 | §1c. Low cost, and 167/168 own this surface next anyway. |
| R12 | **Checklist failure ≠ completion**: on fetch error show a slim "Couldn't load your setup progress — Retry" row instead of hiding the card | 179 (empty/error sweep) | F1.3's silent-hide survived the redesign; 179 is the designated sweep wave. |

---

*Review only — no source files were modified. Checkout items (#1, #5, #6, #7)
were status-confirmed only, per the 181–188 schedule; their detailed re-audit
belongs to the checkout waves and Review #3.*
