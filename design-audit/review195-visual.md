# Wave 195 — Design Review #3: Visual Designer (Brand Coherence)

Scope reviewed (read-only, no source edits): `frontend/src/pages/index.astro`,
`frontend/public/favicon.svg`, wordmark usage in landing / `DashboardNav.astro` /
auth / payer pages, `frontend/src/components/Icon.astro` vs actual icon need,
`android/app/src/main/res/values{,-night}/colors.xml` vs
`frontend/src/styles/tokens.css`, and the Bangla type stack. Evidence is
file:line / grep-verified against the tree at review time.

## Verdict

The brand spine is real and mostly held: one green, one check-tile mark, one
wordmark lockup, tokens-only landing. But coherence breaks exactly where the
user is most exposed to trust cues — **the payer pages drop the logo mark,
their trust-row lock icon renders as an empty SVG, and Android has no
launcher icon at all.** Those three outrank everything else below.

## Passes

1. **One seed green, exactly.** Web `--p-green-600: #0b7a55`
   (`tokens.css:13`) = `--color-brand`, favicon tile fill `#0b7a55`
   (`public/favicon.svg`), Android light `md_primary #0B7A55`
   (`values/colors.xml:8`), and even the merchant default in
   `dashboard/settings.astro:59,62` (`value="#0b7a55"`). The pre-fix Android
   `#0E9F6E` noted in `DESIGN.md:19` is gone from `themes.xml`/`colors.xml`;
   `#0e9f6e` survives only as web `--p-green-500` (decorative step).
2. **Landing is token-clean.** `index.astro` contains no colour literals
   (grep for hex across `frontend/src` outside `tokens.css` returns only the
   settings colour-picker default above); header comment's "tokens only"
   claim holds. One primary per page: green `.btn`, violet reserved for
   eyebrow + step discs (see Drift 5).
3. **Mark = icon, coherently.** The favicon's white check on a green squircle
   (rx 14/64, stroke 7) is the same glyph family as `Icon.astro`'s `check`
   path and the landing verify-card's "Verified by SMS" row
   (`index.astro:73–74`) — the product's core promise is the logo.
4. **Wordmark lockup is standardised on 3 of 5 surfaces.** Landing header +
   footer (`index.astro:47,141`), `DashboardNav.astro:39`, and the auth trio
   (`login/register/forgot-password.astro:11`, as `.auth-brand`) all use
   `<img src="/favicon.svg" width="28" height="28"> MiraclePay` with the
   shared `.wordmark`/`.auth-brand` treatment in `BaseLayout.astro`
   (700, `--tracking-tight`, `--color-ink`, `--text-lg`).
5. **No emoji-as-icon anywhere** in `frontend/src` (grep clean); `Icon.astro`
   is a disciplined set: 24×24, 1.8px stroke, `currentColor`, round caps.
6. **Android shapes mirror web radii:** M3 shape appearances 8/12/16dp
   (`values/themes.xml`) = web `--radius-md/lg/xl` 8/12/16px.
7. **Bangla micro-typography rules exist and fire:** `:lang(bn)` gets
   line-height 1.7, no caps, no letter-spacing (`tokens.css`, final rule);
   `lib/i18n.ts:165` syncs `document.documentElement.lang` on toggle, and the
   one static Bangla string on the landing is correctly tagged
   `<span lang="bn">` (`index.astro:151`).

## Brand drift points (with evidence)

### D1 — CRITICAL: `lock` icon does not exist; payer trust row renders blank
`Icon.astro` defines 24 names (check, copy, wallet, link, chart, key, plug,
settings, shield, bell, box, send, refresh, alert, clock, phone, globe,
logout, search, sun, moon, menu, arrowRight, receipt) — **no `lock`**. Both
payer pages call it anyway: `checkout/[id].astro:216` and
`pay/[slug].astro:120` (`<Icon name="lock" size={15}/>` inside `.trust-row`).
Because `Icon.astro` falls back silently (`paths[name] ?? ""`), the result is
an empty 15px SVG beside "Secure hosted checkout by MiraclePay" — the single
most trust-sensitive line in the product shows a blank gap. Silent failure is
the systemic half of this bug: no build error, no visual placeholder.

### D2 — HIGH: Payer pages drop the logo mark entirely
`checkout/[id].astro:35` and `pay/[slug].astro:24` render
`<p class="wordmark">MiraclePay</p>` — text only, no `<img>`. A customer
arriving from a merchant link sees a different (weaker) brand than the
merchant's dashboard or the landing: no green tile, no check. This is the
surface where mark recognition matters most, and it's the only surface
without the mark.

### D3 — HIGH: Android has no launcher/app icon at all
`AndroidManifest.xml` `<application>` sets `label` and `theme` but **no
`android:icon` / `roundIcon`**, and `res/` contains no `mipmap-*`, no adaptive
icon XML — only `drawable/{dot_*,avatar_bg,ic_copy}.xml`. The favicon check
tile was never ported; the phone home screen shows the platform default.
Brand surface on Android currently starts at the theme colours, not the mark.

### D4 — MEDIUM: Payer wordmark size token is undefined
`BaseLayout.astro:177`: `.payer-top .wordmark { font-size: var(--text-md); }`
— `--text-md` is **not defined in `tokens.css`** (scale jumps
`--text-base-sm` → `--text-body` → `--text-base` → `--text-lg`). The
higher-specificity payer rule wins with an invalid value, so payer wordmarks
fall back to inherited 16px vs the 18px (`--text-lg`) lockup everywhere else —
a subtle size drift on exactly the D2 pages.

### D5 — MEDIUM: Icon set is built but barely used; needs are served by text glyphs
`Icon` is imported by only 3 files (landing, checkout, pay). Of 24 defined
icons, 11 are never referenced anywhere: copy, plug, bell, box, clock, globe,
logout, sun, moon, menu, arrowRight. Meanwhile real needs go un-iconed:
- `DashboardNav.astro` — 10 nav links and the Log out button are text-only,
  despite `logout`, `wallet`, `receipt`, `link`, `key`, `settings`, `plug`,
  `chart` existing and mapping 1:1 onto those links.
- Copy fields (`.copy-field`, BaseLayout) have no `copy` affordance icon.
- Checkout completion uses a raw text `✓` in a disc
  (`checkout/[id].astro:203`, `.completed-check`) instead of `Icon check`;
  dashboard verification copy uses text `✓` (`dashboard/index.astro:710`);
  pagination uses `←`/`→` characters (`transactions.astro:97,99`) instead of
  `arrowRight` (which also has no `arrowLeft` twin).
- `EmptyState.astro` has no icon slot at all — every empty state is text-only
  while `box`/`bell` sit unused.
Net: the *system* is coherent, the *coverage* is not — iconography currently
exists only on the marketing page.

### D6 — MEDIUM: Bangla fonts are named but never loaded
`--font-sans` (`tokens.css`) lists `"Hind Siliguri", "Noto Sans Bengali"` —
6th/7th, after system-ui/Segoe UI/Roboto/Arial — and there is **no
`@font-face`, no Google Fonts `<link>`, and no font file in `public/`**
anywhere. Nothing guarantees either font exists on the client: where it
doesn't (e.g. stock Windows, which ships neither), Bangla falls through to a
platform default (Nirmala UI / system Noto) with different metrics than the
1.7 line-height rule was tuned against. Bangla rendering is therefore
device-lottery, not a brand treatment. Android is blunter: no `fontFamily`
anywhere in the app, so it inherits the OEM font wholesale.

### D7 — LOW: Cross-platform dark/surface greens diverge (by derivation, not by seed)
Light seed matches exactly (Pass 1), but the derived systems drift apart:
web dark brand `--p-dark-brand #34c98e` vs Android night `md_primary
#7ED7AE`; web dark bg `--p-gray-950 #0b1220` (blue-grey) vs Android night
surface `#101413` (green-grey); web light bg `#f6f7fb` vs Android light
surface `#F7FAF8` (green-tinted); web danger `#d92d20` vs Android error
`#BA1A1A`. Each is internally consistent (token layer / M3 tonal palette),
so this reads as "same family, different cousins" side-by-side — acceptable
platform adaptation, but it is derivation drift, not token parity, and
should be a stated decision rather than an accident. Favicon likewise stays
`#0b7a55` in dark browser chrome (fine — tile is self-contained).

### D8 — LOW: Violet accent leaks from eyebrow to filled discs; spec wordmark never landed
`tokens.css` annotates `--color-accent` "marketing only, never interactive",
yet landing step discs are filled violet with white text
(`index.astro` `.step-number`) and needed a dark-mode foreground patch in
`BaseLayout.astro` (`.step-number { color: var(--p-gray-950) }` under dark) —
the patch lives in the layout, not beside the component that creates the
risk. Separately, the original visual direction (`design-audit/04`: two-tone
"Miracle/Pay" wordmark, Manrope display face) was never implemented — the
shipped single-ink system-font wordmark is internally coherent, but the
spec/implementation gap should be closed by retiring the spec, not left open.

### D9 — LOW: Version string drift in visible copy
Landing says v0.0.2 (`index.astro:60,151`); admin panel body still says
"MiraclePay v0.0.1" (`admin/index.astro:26`). Header comments saying v0.0.1
are invisible; the admin one is user-facing brand copy.

## Ranked fixes

| # | Fix | Addresses | Effort |
|---|-----|-----------|--------|
| 1 | Add a `lock` path to `Icon.astro` (or switch payer trust rows to existing `shield`), **and** make unknown icon names fail visibly in dev (console warn / placeholder) — never silent-empty | D1 | S |
| 2 | Give payer pages the full lockup: `<img src="/favicon.svg">` inside `.payer-top .wordmark` on `checkout/[id]` + `pay/[slug]`, matching landing/dashboard | D2 | S |
| 3 | Port the check tile to Android: adaptive icon (green `#0B7A55` background + white check foreground vector) + `android:icon` in the manifest | D3 | S–M |
| 4 | Fix `--text-md`: either add it to the type scale or change `BaseLayout:177` to `--text-lg` so payer wordmark matches the standard lockup size | D4 | XS |
| 5 | Wire the existing set into the dashboard: icons on `DashboardNav` links + `logout` on Log out, `copy` on copy fields, `Icon check` replacing text `✓` discs/copy, `arrowRight` (+ new `arrowLeft`/`chevron`/`x`/`plus`/`download` as needed) for pagination/actions; add an icon slot to `EmptyState` | D5 | M |
| 6 | Actually load a Bangla face: self-host Hind Siliguri (subset, `font-display: swap`) via `@font-face` and move it ahead of generic Latin fallbacks for `:lang(bn)`, or explicitly document system-fallback as the decision; mirror with an Android `fontFamily` choice | D6 | M |
| 7 | Decide and document the dark/surface derivation rule (web tokens vs M3 tonal palettes) in `DESIGN.md`; align at least dark brand green if side-by-side surfaces (webview/help links) are planned | D7 | S (doc) |
| 8 | Retire or implement the 04 spec (two-tone wordmark / Manrope); move the dark `.step-number` foreground rule beside the landing step styles or onto a token (`--color-on-accent`) | D8 | S |
| 9 | Single-source the version string for visible copy (admin panel v0.0.1 → current) | D9 | XS |

## Notes / non-findings
- Gateway wall on the landing remains text chips with no gateway logos or
  brand colours — previously ruled an honest interim (no fabricated logos);
  unchanged and not re-flagged.
- Merchant brand colour on payer pages is a *merchant* surface
  (`settings.astro` brand colour/logo), not MiraclePay drift — the D2 fix
  should keep the MiraclePay lockup distinct from merchant branding.
- Not assessed: rendered screenshots / pixel-level rendering (no visual
  regression run in this review), Android in-app screens beyond the brand
  token/theme layer, and OG/social share imagery (none exists in `public/` —
  worth a future wave, not scored here).
