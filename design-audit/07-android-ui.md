# 07 — Android UI Audit & Material 3 Redesign Spec

**Seat:** Android UI Engineer, 10-agent design engineering team (Waves 151–200)
**Scope audited (read-only, no source edits):**
`android/app/src/main/res/layout/activity_main.xml`,
`android/app/src/main/res/layout/activity_settings.xml`,
`android/app/src/main/res/values/{themes,strings}.xml` (note: **no `colors.xml`, no `dimens.xml`, no `values-night/`, no drawables**),
`MainActivity.java`, `SettingsActivity.java`; supporting data shapes in
`OutboxStore.java`, `StatsStore.java`, `Notify.java`, `build.gradle`, `AndroidManifest.xml`.
**Stack facts that bound the spec:** Views/XML (no Compose), Java 17, `compileSdk/targetSdk 34`,
`minSdk 23`, `material:1.12.0`, `appcompat:1.7.0`, WorkManager 2.9.1. Material 3 is therefore
available *in the already-declared dependency* — the redesign below needs **no new dependency
and no architecture change** (see §3).

---

## 1. Current UI critique

### 1.1 What exists today (accurately)

Two screens, both a single `ScrollView > LinearLayout(vertical, padding=20dp)`:

- **Main:** hand-rolled title (`TextView` 24sp bold) + subtitle, one `MaterialCardView`
  (8dp radius, 2dp elevation, 16dp content padding) holding three `TextView`s, a
  `SwitchCompat`, then a vertical stack of up to **five full-width plain `Button`s**
  (Settings, Test connection, and conditionally-visible Grant SMS permission /
  Retry queued messages / Allow background running).
- **Settings:** label `TextView` + bare `EditText` × 4 (server URL, device key,
  merchant label, allowed senders), a read-only selectable Device ID `TextView`,
  a filled Save `Button` and one `Widget.MaterialComponents.Button.OutlinedButton`
  (Test connection).

All runtime state is rendered by `MainActivity.refreshStatus()` **string-concatenating
into two `TextView`s**: `statusText` gets 3 lines built in one `setText()` plus two later
`append()` calls (Battery, Queue), and `statsText` gets one `·`-separated line plus an
optional second line for the last failure.

### 1.2 Hierarchy

| Problem | Evidence | Impact |
|---|---|---|
| No app bar on Main | `Theme.…NoActionBar`; title is an in-layout `TextView` | No consistent navigation anchor, no overflow/settings affordance, no M3 surface/elevation behaviour, no place for a future sync action. Settings *does* get an AppCompat ActionBar with Up — the two screens use **two different chrome models**. |
| Everything is the same visual weight | Status, config, stats all 14–16sp `TextView`s inside one card; five identical full-width buttons below | The single most important fact — *is forwarding actually working?* — is line 1 of a 5-line paragraph. A merchant cannot answer "am I safe?" at a glance. |
| Primary action is not identifiable | Settings / Test / Grant permission / Retry / Battery are all the same filled `Button` style | The *blocking* actions (permission, battery) look identical to the *routine* ones (Settings). `permissionButton.bringToFront()` is a z-order hack that does nothing meaningful in a vertical `LinearLayout` — intent without effect. |
| Switch is orphaned | `SwitchCompat` sits outside the card, unlabelled as a section, with no supporting text | The master control reads as just another row; its consequence (pause all forwarding) is unexplained. |
| Stats are prose, not data | `"Forwarded: 0 · Failed: 0 · Filtered: 0 · Last success: —"` in one wrapping line | Numbers can't be scanned, compared, or given semantic colour (failures should read as error). Last-failure reason is buried mid-paragraph. |
| Conditional buttons cause layout jump | `permissionButton` / `retryButton` / `batteryButton` toggle `GONE ↔ VISIBLE` from `refreshStatus()` | Content below shifts on every `onResume`; three stacked CTAs appear/disappear with no explanation of *why* they appeared. No banner/card explains the state, the button *is* the explanation — and its label alone ("Allow background running (battery)") doesn't say what breaks without it. |

### 1.3 Spacing & layout system

- **No spacing scale.** Margins in use: 4, 6, 8, 12, 16, 20, 24dp — seven values chosen
  per-view. Page padding is **20dp**, off the M3 4dp grid's standard screen margin (16dp);
  card top margin 24dp vs. section gaps of 16/8dp elsewhere. Nothing is in `dimens.xml`,
  so there is no token to change.
- **No `layout_marginHorizontal`/inset discipline, no max-width** — on tablets/foldables the
  form and buttons stretch edge-to-edge at 20dp padding.
- **Touch targets are incidental.** Buttons inherit the M2 default (~48dp, OK), but the
  switch row relies on `paddingTop/Bottom=8dp` and `TextView` rows (Device ID, status lines)
  that users will try to tap (copy server URL / device ID) are not targets at all.
- **Settings rhythm is label(6dp)field(16dp)** repeated — serviceable, but labels use
  `textAppearanceSubtitle2` (an M2 style) and fields are **bare `EditText`** with the
  platform underline, no box, no helper text, no error slot, no end-icon (e.g. show/hide
  for the device key, paste/clear for the URL).
- Both screens are `ScrollView` even though Main rarely needs to scroll on a tall phone —
  acceptable, but there is no `NestedScrollView` + collapsing bar behaviour and no
  window-insets handling (edge-to-edge / gesture-nav padding is unaddressed; `targetSdk 34`).

### 1.4 Material compliance

- **Theme is Material 2, not M3:** `Theme.MaterialComponents.DayNight.NoActionBar`,
  `colorPrimary`/`colorPrimaryVariant`/`colorSecondary` only. `colorPrimaryVariant` is an
  M2-only role; there are **no M3 roles** (`colorSurfaceContainer*`, `colorOnSurfaceVariant`,
  `colorErrorContainer`, `colorTertiary`, outline roles), no `values-night` overrides, and
  the status bar is forced to `colorPrimaryVariant` (dark green) in both light and dark mode.
- **Brand colour is hard-coded hex in the theme** (`#0E9F6E` primary, `#0B7A55` variant,
  `#FFFFFF` onPrimary) — and, worse, the *same green is reused as `colorSecondary`*, so there
  is no secondary/tertiary accent at all. **No `colors.xml`** exists to name, reuse, or
  dark-mode any of it. Contrast note: white on `#0E9F6E` is ≈3.0:1 — fine for large/filled
  button surfaces per M3's tonal system only if treated as such; it must not be used for
  small body text.
- **Mixed component generations:** M3-era `MaterialCardView` next to `SwitchCompat`
  (AppCompat, not `MaterialSwitch`), platform `Button` (which resolves to M2
  `Widget.MaterialComponents.Button` via the M2 theme), and exactly one explicitly styled
  outlined button in Settings. Main's Test/Settings buttons and Settings' Test button are
  therefore *the same action styled two different ways on two screens*.
- **Typography is ad-hoc `textSize` + `textStyle="bold"`**, not type scale:
  24sp bold title, 16sp status/switch, 14sp everything else. Settings partially uses
  `textAppearanceSubtitle2`/`Body1` (M2 appearances) while Main uses none — inconsistent
  even between the two screens. No `fontFamily`, no headline/title/body/label roles, no
  letter-spacing or line-height control.
- **Shape/elevation:** one radius (8dp) and one elevation (2dp) hard-coded on the single
  card; M3's shape scale (extraSmall 4 / small 8 / medium 12 / large 16) and tonal
  (elevation-by-colour, not shadow) surfaces are unused. Cards in M3 should generally be
  12dp radius, 0–1dp elevation with `surfaceContainerLow` fill.
- **No icons anywhere.** No `res/drawable/`, no vector assets; status is conveyed by text
  only, notifications use the stock `android.R.drawable.stat_notify_sync`
  (`Notify.java`). No leading icons on buttons, no status dot/avatar, no sender monograms
  possible in a future list.
- **No state styling:** no `ColorStateList`s, no disabled/pressed/checked theming beyond
  defaults, no error colour usage — a failed connection and a successful one are both a
  `Toast.LENGTH_LONG`.

### 1.5 Hard-coded values (the debt, enumerated)

- **Strings:** `strings.xml` holds only 7 entries (`app_name` + 5 settings labels).
  *Every* user-visible string on Main is a hard-coded literal in XML
  ("MiraclePay", "SMS payment forwarder", "Forward payment SMS", "Settings",
  "Test connection", "Grant SMS permission", "Retry queued messages",
  "Allow background running (battery)") or, worse, **concatenated in Java**
  (`"Forwarding: " + …`, `"Server: "`, `"Queue: " + pending + " waiting · "`,
  all Toast texts in both activities). This blocks the Bangla i18n the product already
  ships elsewhere, blocks plurals (`1 message` vs `2 messages` — currently the
  `"(s)"` cop-out in `retryOutbox()`'s toast), and makes copy un-reviewable.
  Settings' XML repeats the offence for Merchant label / Allowed senders / Device ID /
  Save / hints, duplicating two labels that *do* exist in `strings.xml`.
- **Dimensions:** every margin/padding/size is an inline dp/sp literal (see §1.3).
- **Colours:** hex only in `themes.xml`; layouts reference `?android:attr/textColorSecondary`
  (framework attr) instead of the Material `?attr/colorOnSurfaceVariant`.
- **Dates:** `SimpleDateFormat("yyyy-MM-dd HH:mm", Locale.US)` — hard-coded pattern/locale,
  not a device-locale relative time ("2h ago"), and not localisable.
- **IDs are the one clean part:** view IDs are stable and consistently named; the redesign
  keeps them (§3) so Java wiring survives.

### 1.6 Feedback, states & accessibility gaps

- **Feedback is Toast-only** for test-connection (both screens, duplicated code),
  save, retry, and permission results. Toasts are transient, un-actionable, not
  announced reliably, and identical for success/failure. No `Snackbar` (no Undo/Retry
  action), no inline result, no progress indicator — Test connection disables its button
  and shows a "Testing connection…" toast; there is **no loading state** in either layout.
- **No empty state:** a fresh install shows "Forwarded: 0 · Failed: 0…" and
  "No server configured yet" (that last string exists only as a `tools:text` preview —
  at runtime it's `"Server: (not set)"`). Nothing guides first-run setup; the path
  *Configure → Grant permission → Test* is never presented as a sequence.
- **No error state:** last failure is one appended line; a failed outbox entry has no
  per-item presence at all — the queue is a **count**, not a list, even though
  `OutboxStore.list()` already returns full `Entry{id, sender, body, receivedAtMillis,
  simSlot, status, …}` objects. The user cannot see *which* bKash/Nagad SMS is stuck,
  cannot retry one item, and cannot distinguish "waiting (offline)" from "failed".
- **Validation is a toast + focus:** bad server URL in `SettingsActivity.saveSettings()`
  toasts and refocuses; the field shows no error, and **Test connection silently tests the
  saved config, not the edited text** — a trap no copy warns about.
- **Accessibility:** no `contentDescription`s needed yet (no images — the one upside),
  but: status conveyed by position in a paragraph (screen-reader users get a run-on
  sentence via `append()`), no headings, Device ID is `textIsSelectable` but has no
  copy affordance/label announcement, switch has no state description beyond its label,
  14sp secondary text on unspecified surfaces risks contrast in dark mode, and nothing
  respects font-scale testing (fixed `ScrollView` will survive it, barely).
- **Dark mode:** `DayNight` parent means it *follows* the system, but with M2 roles,
  a green status bar in both modes, and no night-tuned brand colour, it is un-designed
  rather than designed.

**Verdict:** functional, honest, single-purpose UI — a competent v0.0.1 form — that is
pre-design-system: M2 theme, no tokens, no components beyond card+button+switch,
state-as-paragraph, action-as-toast, and the two highest-value data sets the app owns
(outbox entries, stats) rendered as counts and prose instead of UI.

---

## 2. Material 3 redesign spec

### 2.1 Theme — XML colour roles (the `Color.kt` equivalent for a Views app)

Create `res/values/colors.xml` + `res/values-night/colors.xml`, and re-parent the theme to
**`Theme.Material3.DayNight.NoActionBar`** (available in `material:1.12.0`, already a
dependency). Brand seed: keep the existing green `#0E9F6E` as the seed so the redesign does
not re-brand; derive the tonal roles from it (values below are the spec targets — the
design-systems seat confirms final hexes against the web design tokens).

**Light (`values/colors.xml`)**

| M3 role (theme attr) | Value | Notes |
|---|---|---|
| `colorPrimary` | `#0B7A55` | Darkened from today's `#0E9F6E` so white onPrimary ≥ 4.5:1 for button text |
| `colorOnPrimary` | `#FFFFFF` | |
| `colorPrimaryContainer` | `#A6F2D0` / seed-tone 90 | Status-card "healthy" tint, chips |
| `colorOnPrimaryContainer` | `#002114` | |
| `colorSecondary` | `#4C6358` | No longer a copy of primary (today's bug) |
| `colorOnSecondary` | `#FFFFFF` | |
| `colorSecondaryContainer` | `#CFE9DA` | Stats tiles |
| `colorOnSecondaryContainer` | `#0A1F16` | |
| `colorTertiary` | `#3D6373` | Info/links, "last success" accents |
| `colorTertiaryContainer` | `#C1E8FB` | |
| `colorError` | `#BA1A1A` | Failed counts, field errors |
| `colorOnError` | `#FFFFFF` | |
| `colorErrorContainer` | `#FFDAD6` | Permission/failed banners |
| `colorOnErrorContainer` | `#410002` | |
| `colorSurface` / `android:colorBackground` | `#F7FAF8` | |
| `colorOnSurface` | `#191C1B` | Replaces all `?android:attr/textColorPrimary` |
| `colorSurfaceVariant` | `#DBE5DE` | |
| `colorOnSurfaceVariant` | `#404943` | Replaces all `?android:attr/textColorSecondary` |
| `colorSurfaceContainerLow/Container/High` | `#F1F5F2` / `#EBEFEC` / `#E5EAE6` | Card & tile fills (tonal elevation) |
| `colorOutline` | `#707973` | Outlined buttons, field boxes, dividers |
| `colorOutlineVariant` | `#C0C9C2` | Card stroke, list dividers |
| `colorInverseSurface` / `colorInverseOnSurface` | `#2D3130` / `#EFF1EF` | Snackbar |
| `statusBarColor` / `navigationBarColor` | `?attr/colorSurface` | Drop the dark-green bar; edge-to-edge with insets |

**Dark (`values-night/colors.xml`)** — same attrs, M3 dark tones from the same seed:
primary `#7ED7AE`, onPrimary `#003824`, primaryContainer `#005236`,
onPrimaryContainer `#A6F2D0`, secondary `#B3CCBE`, secondaryContainer `#334B3E`,
error `#FFB4AB`, errorContainer `#93000A`, surface `#101413`, onSurface `#E0E3E1`,
surfaceVariant `#404943`, onSurfaceVariant `#C0C9C2`, containers `#1D211F/#232826/#282D2A`,
outline `#8A938C`, outlineVariant `#404943`. (Dynamic colour: **opt-in later** —
`Theme.Material3.DynamicColors.DayNight` is a one-line parent swap on API 31+, but a
payment-forwarder should keep a recognisable brand surface; spec is static brand first.)

**Shape & component styles (in `themes.xml` / new `styles.xml`):**
`shapeAppearanceCornerSmall=8dp, Medium=12dp, Large=16dp`; default
`MaterialCardView` style = 12dp radius, 0dp elevation, `surfaceContainerLow` fill,
1dp `outlineVariant` stroke. Button styles: one each of
`Widget.Material3.Button` (filled = primary action per screen),
`Widget.Material3.Button.OutlinedButton`, `Widget.Material3.Button.TextButton` —
applied consistently on *both* screens. Switch becomes
`com.google.android.material.materialswitch.MaterialSwitch`. Text fields become
`TextInputLayout` (`Widget.Material3.TextInputLayout.OutlinedBox`) +
`TextInputEditText`. Add `MaterialToolbar` style for a shared top app bar.

### 2.2 Typography (type scale → `textAppearance` styles)

Define/adopt the M3 scale via `Theme.Material3`'s built-in appearances; stop using raw
`textSize`/`textStyle` in layouts:

| Role | Appearance | Used for |
|---|---|---|
| HeadlineSmall (24sp) | `TextAppearance.Material3.HeadlineSmall` | Main screen title in app bar / status headline number context |
| TitleLarge (22sp) | `…TitleLarge` | Screen titles (Settings app bar) |
| TitleMedium (16sp, medium) | `…TitleMedium` | Card/section headings: "Forwarding status", "Queue", "Activity" |
| TitleSmall (14sp, medium) | `…TitleSmall` | List-item sender, tile labels |
| BodyLarge (16sp) | `…BodyLarge` | Switch row label, primary status line |
| BodyMedium (14sp) | `…BodyMedium` | Config summary, list-item time/status |
| BodySmall (12sp) | `…BodySmall` | Supporting/helper text, timestamps |
| LabelLarge (14sp, medium) | `…LabelLarge` | Buttons (automatic), chips |
| LabelMedium/Small | `…LabelMedium` | Stat-tile captions, badges ("SIM 1", "Failed") |

Numbers in stat tiles: `TitleLarge` for the value, `LabelMedium` for the caption —
never the reverse. All layouts reference appearances; **zero `android:textSize` literals**.

### 2.3 Screen-by-screen layout plan

Shared chrome (both screens): `CoordinatorLayout`/`AppBarLayout` + `MaterialToolbar`
(app name or screen title, Settings gear action on Main), surface-coloured status bar,
16dp screen margin (`@dimen/screen_margin`), 4dp-grid spacing tokens in `dimens.xml`
(`space_4/8/12/16/24`, card padding 16dp, section gap 24dp, item gap 8dp).

#### A. MainActivity — restructured into five blocks, same view IDs where they exist

1. **Status card (hero)** — `MaterialCardView` (12dp, tonal). Contents:
   - Row: 12dp status **dot** (drawable, tinted by state: primary-container/● healthy,
     error-container/● blocked, surfaceVariant/● off) + `statusText` as **TitleMedium
     headline only** — one of *Forwarding on / Paused / Needs setup / Blocked*.
     `refreshStatus()` sets the headline + dot tint; it no longer builds a paragraph.
   - Supporting lines as separate labelled rows (icon + label + value):
     `configSummaryText` splits into Server row and Device ID row (Device ID row gets a
     **Copy** text-button — the ID's whole purpose is dashboard registration).
   - The master `MaterialSwitch` (`@id/enableSwitch`, ID kept) moves **inside** this card
     as its footer row, with supporting text "Payment SMS are forwarded to your server
     as they arrive." State description for TalkBack included.
   - Card colour is state-tinted: `primaryContainer` wash when healthy, default
     `surfaceContainerLow` when off, `errorContainer` wash when blocked (see §2.4).
2. **Setup / attention banners (replaces the three conditional CTA buttons)** — a
   `LinearLayout` slot (`@id/bannerContainer`) directly under the status card; Java
   inflates **at most two** banner cards, each = icon + TitleSmall headline + BodyMedium
   explanation + one filled/tonal action button (IDs `permissionButton`,
   `batteryButton` preserved on the buttons themselves):
   - *SMS permission:* "SMS permission needed — MiraclePay can't see payment messages
     without it." → **Grant permission**.
   - *Battery:* "Background running restricted — Android may delay forwards for hours
     on this phone." → **Allow background running**.
   - *Not configured:* "No server connected yet — add your server URL and device key."
     → **Open settings**.
   Banners explain *consequence*, not just action — fixing §1.2's core flaw.
3. **Queue / Outbox section** — section heading "Queue" + count chip; below it a
   **`RecyclerView`** (`@id/outboxList`, new — see §3 for why this is still in-scope)
   with `item_outbox.xml`: sender as TitleSmall with a monogram avatar (first letter,
   `primaryContainer` circle), received time (relative, BodySmall), status chip
   (**Waiting** = secondaryContainer chip; **Failed** = errorContainer chip + reason
   when present), SIM-slot label when >0. Header row carries the existing
   `@id/retryButton` restyled as a tonal button "Retry all", visible only when
   pending+failed > 0 (same rule as today). Empty queue → §2.5 empty state, list hidden.
   Data is already available: `OutboxStore.list()` returns every field this row needs.
4. **Stats section ("Activity")** — 3-up tile row (equal-weight `LinearLayout` or
   `GridLayout`): **Forwarded** (value in primary), **Failed** (value in `colorError`
   when >0, onSurface when 0), **Filtered** — each a `surfaceContainer` tile, value
   TitleLarge + caption LabelMedium. Below: two BodyMedium rows with icons —
   "Last success · <relative time>" and, only when present, "Last failure ·
   <time> — <reason>" in error colour. `@id/statsText` is retired in favour of
   `@id/statsForwardedText/statsFailedText/statsFilteredText/statsLastText`
   (new IDs, set individually — no more string concatenation).
5. **Actions footer** — Settings becomes an **outlined button**, Test connection a
   **text/tonal button** (both full-width is dropped; they sit in one row, or Settings
   moves to the toolbar). Test result is no longer only a toast: an inline result line
   under the button + Snackbar (§2.5). `@id/settingsButton`, `@id/testButton` IDs kept.

#### B. SettingsActivity — same fields, M3 form components

- `MaterialToolbar` with Up (replacing reliance on the AppCompat ActionBar; manifest
  `parentActivityName` already set), title from `@string/settings_title` (exists).
- Each field becomes **`TextInputLayout` (outlined box) + `TextInputEditText`**, IDs
  preserved on the inner edit text (`serverUrlEdit`, `deviceKeyEdit`,
  `merchantLabelEdit`, `allowedSendersEdit`) so `SettingsActivity.java` compiles
  unchanged apart from error calls:
  - *Server URL:* hint becomes the floating label; helper text "Starts with https://";
    keyboard/`inputType=textUri` kept; error slot used by validation (§2.5).
  - *Device key:* password toggle end-icon (`endIconMode=password_toggle`) — the key is
    a secret users must be able to verify; `textPassword` kept.
  - *Merchant label:* optional — label suffix "(optional)", no validation.
  - *Allowed senders:* multi-line box kept (`minLines=3`), helper text replaces the
    parenthetical currently crammed into the label: "One per line. Leave empty to
    forward from all senders."
- **Device ID** becomes a read-only outlined row: label + selectable value + **Copy**
  icon-button (`contentDescription="Copy device ID"`), so registration in the dashboard
  is one tap.
- **Sticky action bar** at the bottom (outside the `ScrollView`, surface container with
  top divider): **Save** = filled button (the screen's single primary), **Test
  connection** = outlined button — the same two styles, in the same order, as specced
  for Main, fixing today's cross-screen inconsistency. Directly above the bar, a
  BodySmall note on Test: "Tests the last saved settings." (documents existing behaviour
  instead of trapping the user).
- All labels/hints/helpers move to `strings.xml`; section grouping: "Server" group
  (URL, key, label), "Filtering" group (senders), "This device" group (ID).

#### C. Notifications (flagged, out of layout scope)

`Notify.java` should gain a brand small icon (vector, `ic_stat_miraclepay`) replacing
`android.R.drawable.stat_notify_sync`, and channel names/descriptions in `strings.xml`.
No layout work; noted so the redesign doesn't ship a stock sync glyph as its face.

### 2.4 Permission & battery states (state matrix for Main)

`refreshStatus()` already computes every input; the redesign is a mapping, not new logic:

| Configured | Permission | Battery | Enabled | Hero headline | Card tint | Banners shown |
|---|---|---|---|---|---|---|
| No | — | — | — | **Needs setup** | default | Not-configured |
| Yes | Not granted | — | — | **Blocked — permission** | errorContainer wash | SMS permission |
| Yes | Granted | Restricted | On | **Forwarding on** | primary wash | Battery (non-blocking warning) |
| Yes | Granted | Unrestricted | On | **Forwarding on** | primary wash | none |
| Yes | Granted | — | Off | **Paused** | default | none (switch explains itself) |
| Yes | Granted | — | On, queue failed >0 | **Forwarding on** | primary wash | Queue section shows Failed chips + Retry all (no extra banner) |

Rule: hero answers *is it working*, banners answer *what must I do*, queue answers
*what is stuck*. No state is expressed by a button appearing without explanation again.

### 2.5 Empty / error / loading states

- **Empty — first run (Main):** when `!isConfigured()`, the stats and queue sections are
  replaced by a single **setup checklist card**: 1 Connect your server (→ Settings),
  2 Grant SMS permission, 3 Send a test — each row checks off as its condition becomes
  true (all conditions already computed in `refreshStatus()`). This is the onboarding
  the app currently lacks entirely.
- **Empty — queue:** centred block in the Queue section: icon (inbox/done), TitleSmall
  "Queue is clear", BodyMedium "Messages that can't be sent yet will appear here."
  Shown when `pending + failed == 0`; replaces today's silent absence.
- **Empty — stats:** before the first forward, tiles show "—" (not "0") and the
  last-success row reads "No messages forwarded yet."
- **Error — field:** server-URL validation sets `TextInputLayout.setError("Enter a URL
  starting with http:// or https://")` and keeps the toast *out*; error clears on text
  change. (Java change is ~5 lines in `saveSettings()`.)
- **Error — connection test:** result renders inline (icon + BodyMedium line under the
  actions, error colour on failure, e.g. "Couldn't reach server (HTTP 403) — check the
  device key") **plus** a `Snackbar` with a **Retry** action on failure. Success =
  primary-coloured inline line "Connected · HTTP 200 · <time>". Toasts are retired
  from both activities.
- **Error — permission denied:** after `onRequestPermissionsResult` denial, the
  permission banner persists and gains a second line: "Denied — you can also enable it
  in Settings → Apps → MiraclePay → Permissions." (Today: one long toast, then nothing.)
- **Loading:** Test connection shows a `LinearProgressIndicator` at the top of the
  actions area (or `CircularProgressIndicator` inside the disabled button) for the
  duration of the existing background thread — the button-disable already exists, only
  the indicator is new. Retry-all shows the same indicator in the Queue header while
  `retryOutbox()` runs. No skeleton screens: the app is local-first, all other data is
  synchronous `SharedPreferences`/store reads, so no other loading state is honest.

---

## 3. What can be done in XML/Java without architecture changes

Everything in §2 is deliberately scoped to the existing two-activity, Views-based,
findViewById architecture. No Compose, no fragments/navigation component, no MVVM
rewrite, no new dependencies (`material:1.12.0` already ships every M3 component named).

**Tier 1 — pure resources/XML, zero Java changes:**
1. Add `colors.xml` + `values-night/colors.xml`, `dimens.xml`; re-parent theme to
   `Theme.Material3.DayNight.NoActionBar`; define shape/text appearances.
2. Move **all** layout literals to `strings.xml`/`dimens.xml` (Main's XML literals and
   Settings' labels/hints). Java-concatenated strings need Tier 2 (below).
3. Restyle both layouts in place: `MaterialToolbar`, `MaterialSwitch`, M3 button styles,
   12dp tonal cards, spacing tokens, stat tiles as separate `TextView`s inside the
   existing card structure. Keeping every existing view ID means `MainActivity` and
   `SettingsActivity` compile and run untouched through this tier (the stats split is
   the one exception — it needs the three new IDs, a ~10-line `refreshStatus()` edit).
4. Add the status-dot and section-icon vector drawables; window-inset padding on the
   root layouts.

**Tier 2 — small, localised Java edits inside the existing methods (no new classes
except where noted):**
5. `refreshStatus()`: set headline/dot/banner visibility from the §2.4 matrix instead of
   `setText`+`append` paragraph-building; banners are pre-declared (initially `GONE`)
   in XML, so this stays visibility-toggling code of the same shape as today's.
6. String resources with **plurals** (`<plurals>` for queue counts and the retry message)
   via `getQuantityString(...)`; relative times via
   `android.text.format.DateUtils.getRelativeTimeSpanString(...)` replacing the
   hard-coded `SimpleDateFormat` — both drop-in inside existing methods.
7. Toast → `Snackbar` swap in `testConnection()`/`saveSettings()` (same callbacks,
   ~15 lines each, and it *deletes* the duplicated toast-building code across the two
   activities if a tiny shared helper is allowed); `TextInputLayout.setError(...)` in
   `saveSettings()`; progress-indicator visibility toggled where buttons are already
   disabled today.
8. Copy-device-ID buttons: `ClipboardManager` + Snackbar confirmation, ~10 lines each.

**Tier 3 — one new component, still no architecture change:**
9. **Outbox `RecyclerView`:** needs `androidx.recyclerview` (transitively present via
   Material/AppCompat in most builds — verify; if absent it's a one-line dependency,
   not an architecture change), one `RecyclerView.Adapter` class (~80 lines) fed by the
   **already-existing** `OutboxStore.list()`, and one `item_outbox.xml`. No data-layer,
   threading, or lifecycle changes: `refreshStatus()` (already called from `onResume`)
   calls `adapter.submitList(outbox.list())`. Per-item retry is a follow-up — v1 of the
   list is read-only + existing Retry-all.

**Explicitly out of scope (would be architecture changes — not required by this spec):**
Compose migration, single-activity/Navigation, ViewModel/StateFlow refactor of
`refreshStatus()`, dynamic colour, predictive back / edge-to-edge enforcement beyond
inset padding, and any change to `ConfigStore`/`OutboxStore`/`StatsStore` APIs —
the redesign consumes exactly the getters and lists that exist today.

**Sequencing for the build waves:** Tier 1 (theme/tokens/strings) → Tier 2 states &
feedback → Tier 3 queue list → Settings `TextInputLayout` conversion (touches the most
XML; do it after the token foundation lands). Each tier is independently shippable and
CI-verifiable with the existing build.

---

*Evidence base: files listed in the header, read in full at audit time (Wave 151).
No source files were modified by this audit.*
