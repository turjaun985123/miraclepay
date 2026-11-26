# 04 — Visual Design Audit — MiraclePay Design Program (Waves 151–200)

**Seat:** Visual Designer, 10-agent design engineering team
**Scope audited (read-only, no source edits):** `frontend/src/pages/index.astro` (landing),
`frontend/src/pages/login.astro`, `frontend/src/pages/register.astro`,
`frontend/src/layouts/BaseLayout.astro`; supporting checks of
`frontend/src/components/DashboardNav.astro`, `frontend/src/lib/i18n.ts`, the rest of
`frontend/src/` for assets/icons/fonts, `frontend/package.json`, and the (non-existent)
`frontend/public/` directory.
**Method:** source inspection only — hex values, class names and copy below are quoted from
the files above. No page was rendered; judgements about hierarchy/whitespace are inferred
from the CSS as written.

---

## 1. Current brand assessment

### 1.1 Verdict in one line

There is no brand yet — there is a competent neutral dashboard design system
(`BaseLayout.astro`) and three public pages that each ignore it in a different colour.
The product reads as "well-built open-source tool," not as a payments brand a Bangladeshi
merchant would recognise, remember, or trust at a glance.

### 1.2 Logo / wordmark treatment — **absent, and inconsistent where it appears**

- **No logo mark exists.** No SVG, PNG, or icon file anywhere in `frontend/src/`; there is
  no `frontend/public/` directory at all, hence **no favicon**, no apple-touch-icon, and no
  Open Graph / Twitter image (no `og:` tags in `BaseLayout.astro` either). Every browser
  tab, bookmark, and social share of MiraclePay is currently blank/generic.
- The wordmark is plain styled text, rendered **three different ways on three pages**:
  | Page | Treatment |
  |---|---|
  | Landing (`.logo`) | 1.375rem, weight **800**, letter-spacing −0.02em, colour `inherit` (near-black) |
  | Register (`.brand`) | weight **700**, letter-spacing **+0.02em**, colour **teal `#0f766e`** |
  | Login | **No wordmark at all** — the card opens directly with the H1 "Log in to MiraclePay" |
- No lockup rules, no clear-space/min-size notion, no monogram to reuse in the Android app,
  checkout, or invoice PDFs. "MiraclePay" set in the system font at weight 800 is
  indistinguishable from a heading.
- Stale versioning leaks into the brand surface: the landing eyebrow and footer both say
  **"MiraclePay v0.0.1"** while `frontend/package.json` is at **0.0.2** — the first thing a
  visitor reads is a version number, and it is already out of date.

### 1.3 Colour story — **four competing primaries, no story**

The single most damaging visual finding. Each public surface picked its own primary:

| Surface | Primary | Hover / focus |
|---|---|---|
| Design system (`BaseLayout`) | Green `#0b7a55` (`--brand`), dark `#095c40`, soft `#e6f6ef` | — |
| Landing (`index.astro`, page-scoped CSS, ignores the tokens) | **Violet `#6d28d9`** — primary buttons, eyebrow, step-number discs | none defined for `.btn-primary` |
| Login (`login.astro`) | **Blue `#2563eb`** — submit button | hover `#1d4ed8`, focus outline `#2563eb` |
| Register (`register.astro`) | **Teal `#0f766e`** — wordmark, button, focus | hover `#115e59` |

A visitor's journey is therefore **violet → teal → blue → green** (landing → register →
login → dashboard). No colour is ever repeated enough to become "MiraclePay's colour."

Supporting neutrals are also forked three ways: the layout uses ink `#101828`, muted
`#667085`, border `#e4e7ec`, bg `#f6f7fb`; the landing re-hardcodes Tailwind-gray
`#e5e7eb / #d1d5db / #f9fafb`; both auth pages re-hardcode Tailwind-slate
`#e2e8f0 / #cbd5e1 / #64748b / #475569`. Same visual intent, three palettes, zero shared
tokens on the public pages.

Other colour notes:

- The one deliberate colour decision in the codebase is good: `--brand` was darkened in
  Wave 23 from `#0e9f6e` to `#0b7a55` for WCAG AA contrast of white button text (≈5:1).
  That green is the natural brand anchor — and the landing page doesn't use it.
- **No dark mode**: `<meta name="color-scheme" content="light">`, no dark token set.
- Gateway brand colours (bKash pink, Nagad orange, etc.) are **not used anywhere** — the
  13 gateways are rendered as identical grey text chips (see §2.2).
- Semantic colour (success `#ecfdf5/#065f46` note on login, error `#fef2f2/#b91c1c` on both
  auth pages) is serviceable but again locally re-invented rather than taken from
  `--brand-soft / --danger-soft` in the layout.

### 1.4 Imagery — **none**

Zero images of any kind: no product screenshot, no dashboard or phone mockup, no
illustration, no photography, no background pattern or gradient (the hero is flat
`--bg`-less white/inherit). For a payments product whose core magic — *"customer sends
money, an SMS arrives, the payment verifies itself"* — is inherently visual and sequential,
nothing is ever shown. The visitor must imagine the product from prose alone. This is the
landing page's biggest conversion weakness after the colour split.

### 1.5 Iconography — **none; and, credit where due, no emoji either**

- **No icon set, no inline SVG, no icon font anywhere in `frontend/src/`.** The eight
  landing feature cards are headline + paragraph only; the dashboard nav is text-only
  pills; buttons never carry icons.
- **Emoji-as-icon usage: none found.** A sweep of all `.astro`/`.ts` sources found no
  pictographic emoji used as icons (or at all) on the landing, login, register, or layout.
  This is a genuine positive — the codebase avoided the cheapest shortcut, so an SVG set
  can be introduced cleanly with no emoji debt to unwind.
- The only glyph-as-icon devices in the wider UI are typographic: the **`✓`** character
  (checkout's `.completed-check`, and the dashboard string "Verified ✓"), **`→`** arrows,
  and the landing's numbered step discs (1/2/3 in violet circles — the one repeated visual
  motif on the page, currently in the wrong brand colour). These should be absorbed into
  the proposed SVG set (§3.4) rather than left as font-dependent glyphs whose rendering
  varies by platform.

### 1.6 Typography (current state)

- One stack for everything: `system-ui, -apple-system, "Segoe UI", Roboto, …` (`--font`),
  plus a system mono. **No webfont is loaded anywhere** — no display face, no brand voice
  in type; headings are just heavier system text (layout weight 650; landing logo 800).
- **Bangla is structurally supported but typographically unserved.** `i18n.ts` ships real
  bn translations and a `বাংলা` toggle exists — but only inside `DashboardNav` (i.e. after
  login). The landing page has **no language toggle and no Bangla copy at all**, despite
  the headline "Payments for Bangladesh and beyond." And the font stack names **no Bengali
  font** (no Hind Siliguri, no Noto Sans Bengali), so Bangla rendering falls back to
  whatever each device happens to have — inconsistent shaping/weight on low-end Android,
  which is precisely the target merchant's device class.
- Landing type scale is the one bright spot: hero `clamp(2.25rem, 6vw, 3.75rem)`,
  line-height 1.05, tracking −0.03em is a modern, confident display setting — wasted on a
  duplicated headline (see §2.1).

---

## 2. Landing page critique, section by section

Page structure as shipped: header/nav → hero → gateways → features → how-it-works →
pricing → footer. All sections are single-column, max-width 72rem, separated only by
`padding: 2.5rem 0` — no background alternation, no dividers, no visual rhythm changes,
so the page reads as one long undifferentiated document.

### 2.1 Header + Hero

- **Nav** is minimal to the point of empty: wordmark, Features, Gateways, Login, and a
  "Get started" button. No Docs, no GitHub link (remarkable for an AGPL open-source
  product whose GitHub presence *is* its trust signal), no language toggle.
- **Eyebrow wastes its slot**: "MIRACLEPAY V0.0.1" in tracked-out uppercase violet tells
  a merchant nothing. This slot should carry the differentiator (e.g. "Self-hosted ·
  No per-transaction fee").
- **The headline says the same sentence three times.** Page `<title>`, H1, hero sub, and
  meta description are all variants of *"Payments for Bangladesh and beyond — accept
  bKash, Nagad, Rocket, cards and more, verified automatically by SMS."* The H1 is a
  category label, not a benefit; the genuinely magical claim — **automatic SMS
  verification, no screenshots, no manual checking** — is buried as the tail of the sub.
- **Text-only, centre-aligned, no visual anchor** (see §1.4). Nothing to look at, nothing
  that demonstrates the product, no checkout mock, no phone, no "verified" moment.
- CTAs ("Get started" / "Explore features") are reasonable, but the primary is violet
  (off-brand vs. the design system), has **no hover state**, and the reassurance note
  under them ("Self-hosted, free, and open source under AGPL-3.0") is set at
  `opacity: 0.75` — trust copy rendered to look like a footnote.
- No proof of any kind in the hero: no stats, no stars, no "13 gateways" count — even
  though the raw material exists further down the page.

### 2.2 Gateways

- Thirteen gateways (bKash, Nagad, Rocket, Upay, SureCash, UPI, Paytm, PhonePe, Razorpay,
  Easypaisa, JazzCash, Stripe, PayPal) rendered as **identical grey pill chips with plain
  text names**. This is the section with the highest recognition potential on the page —
  a Bangladeshi merchant scans for the bKash name/colour the way one scans for Visa —
  and it is given the least visual treatment. No gateway logos, no brand colours, no
  grouping (Bangladesh mobile banking / India UPI / Pakistan / cards), no logos-wall
  layout. As-is it reads as a tag list from a blog post.
- Heading hierarchy is flat: "Gateways" + one-line sub, same weight as every other
  section; nothing signals this is a headline strength (breadth across BD/IN/PK + cards).

### 2.3 Features

- Eight cards in an undifferentiated `auto-fit` grid, **no icons**, equal weight, each a
  title + 3-line paragraph. Hosted checkout, payment links, and SMS verification (the
  three a merchant cares about) sit at the same visual rank as settlements and AGPL
  licensing (which developers/operators care about). No grouping into
  *Sell / Verify / Build* audiences, no bento-style emphasis card for the SMS engine —
  the product's signature feature gets exactly 1/8 of the grid.
- Card styling is a 1px `#e5e7eb` border, 0.75rem radius, no shadow, no hover — flatter
  and greyer than the design system's own `.card` (which has `--shadow` and `--radius`),
  because the landing re-implements cards locally instead of reusing them.

### 2.4 How it works

- The strongest section conceptually: 3 steps (create payment → customer pays by
  send-money → SMS verifies, webhook fires) with numbered discs — the only motif on the
  page. But the discs are **violet**, the steps are disconnected boxes (no connecting
  line/arrow flow, despite being a sequence), and there is still no visual of the actual
  artefact (checkout screen, incoming SMS, "Paid" state) that would make step 3's magic
  believable. This section, visualised, *is* the hero this page needs.

### 2.5 Pricing & social proof

- **Pricing is a paragraph, not pricing.** One centred block of prose ("self-hosted,
  free, AGPL-3.0… no monthly fee, no per-transaction cut") + a CTA. The offer is
  genuinely strong — **৳0 / $0 forever, 0% commission** — and it is formatted to be
  skimmed past. No price figure, no card, no comparison against hosted gateways'
  percentage cuts, no "what you still pay" honesty table (hosting + gateway charges),
  which for an open-source product would build more trust than the claim alone.
- **Social proof: entirely absent.** No testimonials, no merchant logos/names, no usage
  numbers, no GitHub stars/forks badge, no stats band. Some of this is honest absence
  (a v0.0.x product should not fabricate quotes — and this audit does **not** propose
  inventing any), but verifiable proof already exists and is unused: 13 gateways,
  118-endpoint API (per project records), AGPL-3.0 licence, GitHub repo itself.
  The page currently asks for trust while showing none of its receipts.

### 2.6 Footer

- Two centred lines ("MiraclePay v0.0.1" / "Self-hosted, free, AGPL-3.0.") + the same
  four nav links. No columns, no Docs/API/GitHub/licence links, no language switch, no
  wordmark lockup, no back-to-top. For a self-hosted product, the footer is where
  operators look for docs, releases, and licence — all missing. It closes the page with
  the same stale version string it opened with.

### 2.7 The auth pages as brand surfaces (login / register)

Login and register are a merchant's first *product* impression, and they currently
contradict each other: teal wordmark + teal button (register) vs. no wordmark + blue
button (login), slate neutrals on both vs. the layout's grey-green system, different
focus colours (`#0f766e` vs. `#2563eb`), different card widths (26rem vs. 24rem) and
shadows. Positives to keep: the register placeholder **"Ayesha Rahman"** is a small,
correct localising touch; both cards are clean and uncluttered; error/success colours are
legible. Neither page has a brand panel, product imagery, or a single line of reassurance
("your payment data stays on your server") where a self-hosted product most needs it.

---

## 3. Proposed visual direction

### 3.1 Brand palette refinement — one green, disciplined accents

**Decision: keep the design system's green as the brand colour; kill the other three
primaries.** Rationale: it is already tokenised (`--brand`), already AA-checked
(Wave 23), it carries money/trust semantics, and it resonates locally (deep green is
Bangladesh's national colour family) without copying the flag. Violet `#6d28d9`, login
blue `#2563eb`, and register teal `#0f766e` are retired as primaries — every CTA, focus
ring, wordmark accent, and step disc across landing/login/register/dashboard maps to the
one scale below.

| Token | Value | Role |
|---|---|---|
| `--brand-700` | `#095c40` (existing `--brand-dark`) | Wordmark accent, links, hover/pressed |
| `--brand-600` | `#0b7a55` (existing `--brand`) | Primary buttons, key icons, step discs |
| `--brand-500` | `#0e9f6e` (pre-Wave-23 green) | **Decorative only** — chart accents, gradient end, large graphics; never small text (fails AA) |
| `--brand-100` | `#e6f6ef` (existing `--brand-soft`) | Icon tiles, active nav, success surfaces |
| `--brand-50` | `#f2faf6` *(new)* | Hero tint / alternating section background |
| `--accent-500` | `#f59e0b`-family amber *(new, sparing)* | The single warm accent: "Free / ৳0" pricing badge, highlights, the check inside the logo mark. Never a button colour. |
| `--ink` / `--muted` | `#101828` / `#667085` (existing) | Text — adopt layout values everywhere; delete the landing's gray and auth pages' slate forks |
| `--bg` / `--surface` / `--border` | `#f6f7fb` / `#ffffff` / `#e4e7ec` (existing) | Surfaces — same: one set, no per-page re-hardcoding |
| Semantic | success = brand scale; warning `#b54708/#fffaeb`; danger `#d92d20/#fef3f2` (all existing) | Reuse; auth pages stop re-inventing them |

Rules: (1) gateway brand colours (bKash pink `#e2136e`-family, Nagad orange, etc.) may
appear **only** inside gateway chips/logos as identification, never as UI chrome;
(2) one dark surface is introduced for the developer/code section and future dark mode —
ink `#0c111d` with brand-500 accents; (3) hero may use a soft brand-50→white gradient
plus a faint geometric pattern (§3.5), everything else stays flat.

**Logo direction (in words):** a squircle (rounded-square) tile in brand-600 containing
a bold white **check-stroke that doubles as the upright of a "৳"/bolt-free "M"** — the
check is the product (verification) in one glyph; amber dot or check-tip as the single
accent. Wordmark lockup: "Miracle" in ink, "Pay" in brand-700, set in the display face
(§3.2) at weight 800, tracking −0.02em; the tile alone serves as favicon/app icon.
Deliverables: SVG master, favicon set, OG image template using the lockup + hero line.

### 3.2 Typography pairing — Latin + Bangla as first-class

| Role | Latin | Bangla (`lang="bn"` / `:lang(bn)`) | Notes |
|---|---|---|---|
| Display / headings | **Manrope** 700/800 (geometric, fintech-modern, matches the 800-weight wordmark instinct already in `.logo`) | **Hind Siliguri** 600/700 | Hind Siliguri's geometric construction pairs naturally with Manrope; use for bn headings, hero, buttons |
| Body / UI | **Inter** 400/500/600 (neutral, superb at small sizes, tabular figures available) | **Noto Sans Bengali** 400/500/600 | Noto Sans Bengali for bn body/long text — more complete glyph coverage and calmer texture than Hind Siliguri at paragraph sizes |
| Mono / data | keep system mono stack, add `font-variant-numeric: tabular-nums` for amounts | same | Amounts, API keys, transaction IDs |

Implementation principles (for the Design Systems seat): **self-host subsetted woff2**
(a Google-Fonts CDN dependency contradicts the self-hosted/offline ethos and adds a
third-party request to a payments page); `font-display: swap` with the current system
stack as fallback so nothing regresses before fonts load; define `--font-display`,
`--font-body`, `--font-bn-display`, `--font-bn-body` tokens and switch stacks on
`document.documentElement.lang` (which `i18n.ts` already maintains).

Bangla-specific rules, non-negotiable:
- **Line-height up** for bn body (≈1.7 vs. 1.55) — Bengali conjuncts/ascenders need the air.
- **No uppercase + letter-spacing on Bengali**: the landing eyebrow pattern
  (`text-transform: uppercase; letter-spacing: 0.08em`) must be disabled under
  `:lang(bn)` — tracking breaks Bengali shaping and uppercase doesn't exist in the script.
- Test the **৳ (taka) sign**, Bengali numerals (০১২৩…), and conjunct-heavy words
  (e.g. "স্বয়ংক্রিয়ভাবে") in both fonts at button sizes before sign-off.
- Bring the language toggle to the **landing header and auth pages**, not just the
  post-login dashboard nav where it lives today.

### 3.3 (Reserved — layout rhythm)

Adopt from the layout, don't reinvent: `--radius 12px`, `--shadow`, section alternation
white / brand-50, a real type scale (display 3.75rem → h2 1.875rem → h3 1.125rem), and
8-pt spacing. The landing must consume `BaseLayout` tokens/classes (`.card`, `.btn`)
instead of its current page-scoped forks.

### 3.4 Inline-SVG icon set — 24 icons

Style spec: single inline-SVG sprite (`<symbol>` set) or an `Icon.astro` component;
**24×24 viewBox, 1.8px stroke, round caps/joins, `currentColor`, no fills** (except the
active-nav state, which uses brand-100 tile + brand-700 stroke); `aria-hidden="true"`
with text labels always adjacent — icons never stand alone as the only label. This set
replaces the `✓`/`→` glyph hacks and gives the feature grid, dashboard nav, and steps
their missing visual vocabulary:

| # | Icon | Used for |
|---|---|---|
| 1 | `logo-check` (brand mark, §3.1) | Wordmark lockup, favicon, loading/empty states |
| 2 | `grid` (overview) | Dashboard nav — Overview |
| 3 | `arrows-exchange` | Dashboard nav — Transactions |
| 4 | `link` | Payment Links; copy-link actions |
| 5 | `receipt` | Invoices |
| 6 | `wallet` | Gateways / balances |
| 7 | `plug` | Gateway & plugin connections |
| 8 | `webhook` (hook + nodes) | Webhooks |
| 9 | `key` | API Keys |
| 10 | `gear` | Settings |
| 11 | `shield-check` | SMS verification, security, 2FA, trust copy |
| 12 | `message-sms` (speech bubble + lines) | SMS verification engine — the signature feature |
| 13 | `checkout-cart` | Hosted checkout feature |
| 14 | `check-circle` | Paid / verified states (replaces `✓` glyph) |
| 15 | `clock` | Pending states |
| 16 | `x-circle` | Failed states |
| 17 | `bell` | Notifications |
| 18 | `search` | Transaction/product search |
| 19 | `plus` | Create actions |
| 20 | `copy` | Copy number / reference / API key |
| 21 | `download` | Self-host download, exports |
| 22 | `code` (`</>`) | REST API / developer section |
| 23 | `server` | Self-hosted feature, settlements hosting copy |
| 24 | `globe` | Language toggle (বাংলা / English) |

(Runner-up, add when nav grows: `chart-bar`, `bank`, `phone`, `eye / eye-off`,
`external-link`, `chevron-down`, `menu`, `log-out`, `sun / moon` for dark mode.)

### 3.5 Hero & marketing layout concepts (in words)

**Concept A — "The verification moment" (recommended hero).** Split hero: left column
carries a rewritten benefit headline (SMS auto-verification as the claim, not the
category), one-line sub, the two existing CTAs recoloured to brand green, and the AGPL
note promoted to a proper trust row with `shield-check` + `server` icons. Right column
is a **built-in-code product visual** (HTML/CSS mock, no screenshot dependency, themeable
later): a phone frame showing a bKash send-money screen beside a checkout card whose
status flips to a green `check-circle` "Verified by SMS" toast. This shows the one thing
no competitor screenshot can: the payment confirming itself.

**Concept B — Centred hero + product stage.** Keep the current centred composition (it
suits a developer-tool audience) but fix it: eyebrow becomes the differentiator line,
H1/sub stop duplicating each other, and directly beneath the CTAs sits a wide browser-frame
mock of the dashboard with a payment row mid-verification. Below the fold it acts as the
page's first proof object.

**Concept C — Bangla-first variant.** Same layout as A, designed bn-first rather than
translated into it: Hind Siliguri display headline, Bengali-numeral stats, gateway wall
led by bKash/Nagad. Ships with the landing language toggle (§3.2); designing C explicitly
prevents the bn page becoming a stretched afterthought.

**Supporting section redesigns (words only):**
- **Gateway wall:** grouped logo wall — three labelled groups (Bangladesh · India ·
  Pakistan & Cards); interim treatment before official logo assets are cleared: gateway
  names set in Manrope 700 inside soft tiles, each tile's left edge or dot in that
  gateway's brand colour. Identification colour, neutral chrome.
- **Stats band:** full-width brand-700 strip, four verifiable numbers only — 13 gateways,
  3 steps to verified, 0% commission, 100% self-hosted/AGPL. No invented user counts.
- **Features → bento:** SMS-verification card spans double width with the `message-sms`
  icon and a mini 3-state visual (SMS arrives → matched → Paid); remaining features in
  standard icon-tile cards grouped *Accept / Verify / Build & Run*.
- **How-it-works → connected timeline:** the three existing steps on a horizontal brand
  line with numbered brand discs and `→` replaced by the line itself; each step gets its
  icon (`plus`/`wallet`/`shield-check`).
- **Developer strip:** the one dark section (ink background) — `code` + `webhook` icons,
  a short signed-webhook code sample in mono, link to API docs. Gives the page rhythm and
  speaks to the self-hoster audience the footer currently ignores.
- **Pricing → a price, visually:** single centred card, giant "৳0" with the amber
  "Free forever" badge, checklist of what's included (all gateways, SMS verification,
  webhooks, API) vs. an honest "you provide" list (a server, your gateway accounts).
  Optionally a slim comparison row vs. hosted per-transaction pricing — figures to be
  verified by the UX Research seat before publication.
- **Social proof → honest placeholders:** testimonial card *design* (quote, name, shop,
  city) specced and left empty until real merchants can be quoted; until then the stats
  band + a GitHub/AGPL trust row carry the proof load. **Do not fabricate quotes or logos.**
- **Footer → real footer:** four columns (Product, Developers, Project, Legal) with the
  wordmark lockup, GitHub/docs/licence links, and the language toggle; version string
  moves here only, small, and generated — not hand-typed in two places.
- **Auth pages:** one shared treatment — left brand panel (brand-700 background, logo
  lockup, one reassurance line, gateway dots) + right form card, collapsing to a centred
  card with the lockup on mobile; both pages on brand-green buttons/focus, retiring the
  teal/blue split. Keep the "Ayesha Rahman" placeholder.

### 3.6 Priorities

- **P0 (identity):** adopt the single green scale on landing/login/register; logo mark +
  favicon + OG image; self-hosted Manrope/Inter + Hind Siliguri/Noto Sans Bengali with
  `:lang(bn)` rules; landing language toggle.
- **P1 (conversion):** Hero Concept A, gateway wall, stats band, bento features,
  pricing card, connected how-it-works timeline.
- **P2 (depth):** 24-icon sprite rollout to dashboard nav, developer dark strip, full
  footer, dark-mode token set, testimonial slots once real quotes exist.

---

*Audit only — no source files were modified. All hex values, class names, and copy quoted
above are taken verbatim from the files listed in the scope note.*
