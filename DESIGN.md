# MiraclePay Design System — DESIGN.md

Wave 151 synthesis of the 10-agent design engineering team audit
(full audits: `design-audit/01..10-*.md`). This document is the
contract for Waves 151–200 (v0.0.3 design release).

## Team (Wave 151 audit, 10/10 agents)

Design Lead · UX Researcher · Design Systems Engineer · Visual
Designer · Dashboard UI Engineer · Checkout UX Engineer · Android UI
Engineer · Motion/Interaction Designer · Accessibility Engineer ·
QA/Visual Regression Engineer. Reviews reconvene at Waves 165, 180, 195.

## Headline findings

1. **Four competing brand primaries.** System green `#0b7a55`
   (dashboard), landing violet `#6d28d9`, login blue `#2563eb`,
   register teal `#0f766e` — the user journey changes colour at
   every step. Android still uses the pre-WCAG-fix `#0E9F6E`
   (3.39:1 on white). One product must have one primary: green.
2. **Checkout's core job is impossible from the page alone** (P0):
   neither `/checkout/[id]` nor `/pay/[slug]` ever renders the
   merchant's receiving wallet number for manual payments; checkout
   has no gateway selection (silently uses `gateways[0]`); the pay
   page hard-codes 6 gateways and ends at a static "Payment started"
   with no TrxID fallback, polling, or status.
3. **No app shell.** No header, wordmark, account menu or logout
   anywhere; DashboardNav is a full-width pill strip outside the
   1120px container; merchant switcher exists on only 2 of 8 pages
   while the rest silently act on `localStorage`.
4. **No design tokens beyond 16 layout vars** — 63 hex literals
   (48 outside BaseLayout), 51 inline `style=` attributes, 8 class
   names redefined incompatibly per page, undefined token
   references (`--accent`, `--brand-green`), light-only
   (`color-scheme: light` pinned, no dark mode).
5. **Feedback/motion is near-zero:** 4 ad-hoc transitions, no
   skeletons/toasts/dialogs (grep = 0), "Loading…" text paragraphs
   on 13 pages, five different homes for success feedback, and the
   two pages whose job is *waiting* (checkout/pay) never poll while
   the dashboard polls every 30s.
6. **Accessibility is close but not AA:** danger text on
   danger-soft is 4.44:1 (fix `#b42318` = 6.05:1); input borders
   1.24:1 fail 1.4.11 (needs a `--border-strong` at ≥3:1);
   `.input:focus` outline is 1.12:1 (invisible); merchant
   `brand_color` is painted behind white text with no contrast
   gate; touch targets ~34–43px, none at 44px.
7. **Android is pre-design-system:** M2 theme, no
   colors/dimens/night resources, all state string-concatenated
   into two TextViews, Toast-only feedback, outbox rendered as a
   count although full entries exist, 7 strings in strings.xml
   (everything else hard-coded — Bangla impossible).
8. **Brand surface is empty:** no logo mark, no favicon, no
   `public/`, no imagery, no icon set; landing hero repeats the
   same sentence 4 times, eyebrow still says "v0.0.1", 13 gateway
   chips are identical grey text, pricing's ৳0/0% offer is buried
   in prose, social proof absent.

## Principles (Design Lead, adopted)

1. **One colour.** Green `#0b7a55` is the only interactive
   primary, everywhere, enforced by a CI hex-lint outside
   `tokens.css`. Violet survives only as non-interactive
   marketing accent.
2. **Tokens → components → pages.** No page defines a colour,
   spacing, or radius literal; no component pattern is
   hand-rolled twice (status badges, loading, empty states,
   page headers are shared helpers/components).
3. **One shell per audience.** Public, auth, dashboard/admin
   and payer surfaces each get exactly one shell with header,
   wordmark, and (for dashboard) identity + logout + merchant
   context on every page.
4. **Money pages are the design ceiling.** Checkout/pay get
   the strongest treatment: copyable recipient number, amount
   prominent and sticky, step indicator, live status polling,
   field-level validation, full EN+BN copy.
5. **Bangla is first-class.** Designed Bengali type
   (Hind Siliguri / Noto Sans Bengali in the stack), 1.7
   line-height, no uppercase/letter-spacing under `:lang(bn)`,
   numerals and copy reviewed per surface — not a post-login
   toggle on a subset of strings.
6. **Merchant colour is a generated theme.** Merchant brand
   colour derives hover/soft/on-brand via `color-mix` and is
   contrast-checked (≥4.5:1 against its text colour) at
   settings-save and render time with a safe fallback.
7. **One pattern per state.** Loading = skeleton (150ms
   show-delay), success = toast, destructive = `<dialog>`
   confirm, empty = EmptyState with genuine/filtered/error
   variants. Never optimistic for money movement.
8. **Shared tokens across web + Android.** Android M3 colour
   roles derive from the same green seed; both platforms ship
   light + dark.

## Token system (summary — full spec: design-audit/03)

Three layers in `frontend/src/styles/tokens.css`: primitives
(`--p-*`, the only hex), semantics (`--color-*`, `--space-*`,
`--text-*`, radius/shadow/motion/z — dark mode re-points only
this layer), component knobs. Legacy 16 token names remain as
aliases for one release. Type: 10-step scale, weights 400/600/700.
Spacing: 4px scale. Breakpoints (named constants): 480 / 640 /
900 / 1120px. Motion: 75/120/200/320ms + standard easings,
reduced-motion respected in CSS *and* JS.

## Roadmap

Waves 151–200 as logged in WAVES.md: 151 audit (this document) ·
152–158 design system foundation · 159–168 landing + auth ·
165 team review #1 · 169–180 dashboard · 180 team review #2 ·
181–188 checkout + pay · 189–194 Android · 195 team review #3 ·
196–199 polish (responsive, motion, a11y, consistency) ·
200 v0.0.3 release.
