# Review 195 — Android UI: Material 3 Redesign vs Wave 151 Spec (§2 + §3)

**Seat:** Android UI Engineer, Design Review #3 (Waves 151–200)
**Scope reviewed (read-only, no source edits, no build run):**
`res/values/{colors,themes}.xml`, `res/values-night/{colors,themes}.xml`,
`res/values/dimens.xml`, `res/layout/{activity_main,activity_settings,item_outbox}.xml`,
`MainActivity.java`, `SettingsActivity.java`, `OutboxAdapter.java` — against
`design-audit/07-android-ui.md` §2 (redesign spec) + §3 (Tier 1/2/3).
Checked for context only: `strings.xml`, `res/drawable/*`, `app/build.gradle`,
`OutboxStore.java`, `StatsStore.java`, `AndroidManifest.xml`.
All verdicts below are from reading; **no assemble/CI verdict is claimed by this review.**

**Headline:** The foundation landed almost exactly as specced — M3 theme, exact
spec hexes in both modes, tokens, toolbars, banners, stat tiles, `TextInputLayout`
form, outbox `RecyclerView`. What was dropped is concentrated in §2.4/§2.5 state
polish: no state-tinted hero card, no not-configured banner or setup checklist,
no count chip / status chips, no error colouring of failures, and Settings'
test-connection feedback stayed a step behind Main's. Nothing dropped breaks
the redesign's core promise ("am I safe?" is now answerable at a glance).

---

## 1. Tier checklist (§3)

### Tier 1 — pure resources/XML, zero Java changes

| # | Spec item | Verdict | Evidence / note |
|---|---|---|---|
| 1a | `colors.xml` + `values-night/colors.xml`, re-parent to `Theme.Material3.DayNight.NoActionBar` | ✅ Landed, hex-exact | Every light role matches §2.1 verbatim (`#0B7A55` primary — the ≥4.5:1 darkening of the old `#0E9F6E` was honoured — `#A6F2D0`, `#4C6358`, `#BA1A1A`, `#F7FAF8`, containers `#F1F5F2/#EBEFEC/#E5EAE6`, outlines, inverses). Every dark role §2.1 enumerates also matches (`#7ED7AE`, `#005236`, `#93000A`, `#101413`, …). Secondary is no longer a copy of primary (the §1.4 bug is fixed). |
| 1b | Full M3 role set in theme | ⚠️ Partial | Theme wires 25 roles, but `colorOnTertiaryContainer` is set in neither theme and no `md_on_tertiary_container` colour exists in either `colors.xml`; `colorSurfaceContainerLowest/Highest` and `colorInversePrimary` also unset. Harmless today (M3 defaults fill in; nothing references them) — a token-completeness gap, not a visual bug. Dark tertiary (`#A5CBE2`/`#214B5E`) is not spec-verbatim because §2.1 never gave dark tertiary values; the chosen tones are plausible M3 dark tones from the same family — **acceptable**. |
| 1c | Shape scale 8/12/16dp; default card = 12dp radius, 0dp elevation, `surfaceContainerLow` fill, 1dp `outlineVariant` stroke | ⚠️ Partial | `ShapeAppearance.MiraclePay.{Small,Medium,Large}` = 8/12/16dp ✅ and both cards in `activity_main.xml` hard-code 12dp/0dp/1dp `colorOutlineVariant` stroke ✅. But there is **no default `MaterialCardView` style**, and no card sets `cardBackgroundColor` to `colorSurfaceContainerLow` — cards rely on the M3 theme default fill. Visually near-identical on this palette; the token is not actually applied. Acceptable, fix opportunistically. |
| 1d | `dimens.xml` spacing tokens, 16dp screen margin, 4dp grid | ✅ Landed | `screen_margin` 16dp, `space_4/8/12/16/24`, `card_padding` 16dp, `section_gap` 24dp — exactly §2.3's token set (plus useful `status_dot` 12dp, `avatar_size` 40dp). Layouts consume tokens throughout; no stray dp literals found in the three layouts except the specced 12dp card radii / 1dp dividers. |
| 1e | Move all layout literals to `strings.xml` | ✅ Landed (layouts) | Zero hard-coded `android:text` literals in the three layouts; all copy is `@string/…`, including banner bodies, helpers, and the queue-empty text. Java-side leftovers are Tier 2 — see §3 below. |
| 1f | Status-bar / nav-bar = `colorSurface` (drop dark-green bar) | ✅ Landed | Both themes set `android:statusBarColor`/`navigationBarColor` to `@color/md_surface`; `windowLightStatusBar` true in light, false in `values-night/themes.xml` — correct per-mode behaviour. |
| 1g | Restyle in place: `MaterialToolbar`, `MaterialSwitch`, M3 button styles, stat tiles; keep existing view IDs | ✅ Landed with one deviation | Both screens have a `MaterialToolbar`; switch is a `MaterialSwitch` **in XML** inside the hero card; buttons are `Widget.Material3.Button` (default filled) / `.OutlinedButton` / `.TonalButton`, consistently on both screens. All pre-existing IDs kept (`enableSwitch`, `permissionButton`, `batteryButton`, `retryButton`, `settingsButton`, `testButton`, `serverUrlEdit`, …); `statsText` split into `statsForwardedText/statsFailedText/statsFilteredText/statsLastText` exactly as §2.3A.5 prescribed. Deviation: Java still declares the switch as `SwitchCompat` — see compile-risk R2. |
| 1h | Status-dot + section-icon vector drawables | ⚠️ Partial | Dot drawables landed (`dot_green/grey/red`, simple `?attr`-tinted ovals) and one icon (`ic_copy`). **No section icons, no button leading icons, no toolbar Settings gear** (Settings lives in the actions row instead — §2.3A.5 explicitly allowed the row, so **acceptable**). Dot colours differ slightly from §2.3A.1 (spec: container-tinted dots; shipped: solid `colorPrimary`/`colorOutline`/`colorError`) — reads stronger, **acceptable**. |
| 1i | Window-inset padding on roots (edge-to-edge) | ❌ Dropped | No `fitsSystemWindows`, no inset handling in either layout or activity. With a surface-coloured opaque status bar this is not user-visible today; it becomes a real gap only if edge-to-edge is enforced later (which §3 explicitly put out of scope). **Acceptable for this wave.** |
| 1j | Typography: M3 `textAppearance`s, zero `textSize` literals (§2.2) | ✅ Landed | Zero `android:textSize`/`textStyle` literals in the three layouts (grep-verified). Headline = `TitleMedium`, tile values = `TitleLarge` + `LabelMedium` captions (never reversed, per §2.2), list sender = `TitleSmall`, meta = `BodySmall`. One nit: `statsLastText` is `BodySmall` where §2.3A.4 said `BodyMedium` rows — minor. |

### Tier 2 — small localised Java edits

| # | Spec item | Verdict | Evidence / note |
|---|---|---|---|
| 2a | `refreshStatus()` implements the §2.4 state matrix (headline + dot + banners), no paragraph-building | ⚠️ Partial — matrix simplified | Headline + dot mapping is exactly §2.4 for the four reachable headlines (`Needs setup` / `Blocked — permission needed` / `Forwarding on` / `Paused`) ✅. **But:** (i) hero card is **never state-tinted** — §2.4's `primaryContainer` wash (healthy) / `errorContainer` wash (blocked) column has no Java or XML implementation; only dot + text change. (ii) Banner visibility does **not** follow the matrix: `permissionBanner` shows whenever SMS is ungranted *including when unconfigured*, and `batteryBanner` shows whenever not exempt *regardless of configured/granted/enabled* — on a fresh install three problems stack at once, where §2.4 shows exactly one (not-configured) banner and §2.3A.2 caps banners at two. (iii) The **not-configured banner does not exist** (no view, no string). Headline still answers "is it working?", so the core fix landed; the matrix's discipline did not. See Fix F2. |
| 2b | Config summary split into Server row + Device-ID row, Copy button on Main (§2.3A.1) | ❌ Dropped on Main | `configSummaryText` is still one `TextView` filled by `"\n"` concatenation of Server + Device ID — the §1 pattern in a new coat. Copy landed only in Settings (2e). Impact is low (the ID is copyable one screen away) but this was an explicit §2.3 item, not polish. See Fix F5. |
| 2c | Plurals via `getQuantityString`; relative times via `DateUtils` (§2.5/§3.6) | ✅ Landed | `R.plurals.retrying_messages` used in `retryOutbox()` (the old `"(s)"` cop-out is gone); `DateUtils.getRelativeTimeSpanString` in both `MainActivity.formatStatsTime()` and `OutboxAdapter`. The hard-coded `SimpleDateFormat("yyyy-MM-dd HH:mm")` is gone from the scoped files. |
| 2d | Toast → Snackbar; inline test result + progress; field `setError` | ⚠️ Partial — Main ✅, Settings ⚠️ | **Main:** fully landed — `LinearProgressIndicator` (`testProgress`) while testing, durable inline `testResultText` coloured primary/error, Snackbar **with Retry action** on failure, no Toasts remain. **Settings:** Snackbar swap ✅ and `serverUrlLayout.setError(...)` + clear-on-text-change ✅ (exactly §2.5), but Test connection there is still Snackbar-only — no progress indicator, no inline result, no Retry action — and its javadoc still says "toasts the outcome" (stale comment). The §1.4 cross-screen inconsistency is reduced, not eliminated. See Fix F4. |
| 2e | Copy-device-ID (ClipboardManager + Snackbar) | ✅ Landed (Settings) | `copyDeviceIdButton` with `ic_copy`, `contentDescription=@string/copy_device_id`, ClipboardManager + "Copied" Snackbar. Form differs from spec (plain selectable `TextView` + icon button, not a read-only outlined row) — functionally identical, **acceptable**. |
| 2f | Permission-denied follow-up copy (§2.5) | ✅ Landed | Denial shows `banner_permission_denied` ("Denied — you can also enable it in Settings → Apps → …") via Snackbar and the banner persists. Delivered as a Snackbar rather than a second line *inside* the banner — transient where the spec wanted durable, minor. |
| 2g | Hard-coded strings out of Java | ⚠️ Mostly | Scoped activities are almost entirely `R.string`-driven. Leftovers: Snackbar action label `"Retry"` (Main), `"SMS permission granted."`, the battery-fallback Snackbar sentence (Main), `"SIM " + slot` (Adapter), `"(HTTP " + code + ")."` fragments (both activities). Small, but they block the Bangla i18n §1.5 cared about. See Fix F6. |

### Tier 3 — Outbox RecyclerView

| # | Spec item | Verdict | Evidence / note |
|---|---|---|---|
| 3a | `RecyclerView` + adapter fed by existing `OutboxStore.list()`, one `item_outbox.xml`, read-only + existing Retry-all | ✅ Landed | `recyclerview:1.3.2` is now an explicit dependency in `build.gradle` (the §3 "verify transitive" risk is closed); `OutboxAdapter` (~110 lines) is a clean read-only adapter; `refreshStatus()` calls `submitList(entries)` from `onResume` as specced; Retry-all keeps its ID, becomes a tonal button, and keeps the `pending+failed > 0` visibility rule. No store/threading changes. |
| 3b | Row contents: monogram avatar, sender TitleSmall, relative time, SIM label, **status chip** (Waiting = secondaryContainer, Failed = errorContainer **+ reason**) | ⚠️ Partial | Monogram ✅ (`avatar_bg` = `colorPrimaryContainer` oval, initial in `colorOnPrimaryContainer`), sender ✅, relative time ✅, `SIM n` in meta line ✅, body snippet added (not specced — genuinely useful, **acceptable addition**). **Status is not a chip:** it is a bare `LabelMedium` `TextView` coloured `colorPrimary` (waiting) / `colorError` (failed), with no container background — and **`Entry.lastError` is never read**, so a failed row never says *why* it failed, which §2.3A.3 explicitly required ("+ reason when present"). See Fix F3. |
| 3c | Queue header: heading + **count chip**; empty state = icon + "Queue is clear" title + body | ⚠️ Partial | Heading ✅, Retry-all ✅; **count chip missing** (no chip view anywhere). Empty state is a single `BodyMedium` line using only `queue_clear_body` — the `queue_clear_title` string ("Queue is clear") exists in `strings.xml` but is **never referenced**, and there is no icon/centred block. Also, emptiness is keyed on `entries.isEmpty()` rather than §2.5's `pending+failed == 0` — equivalent for what `OutboxStore` actually stores today. See Fix F3. |

### §2 items outside the tiers

| Spec item | Verdict |
|---|---|
| §2.3A.4 Stats as a separate "Activity" section with `surfaceContainer` tile fills; Failed value in `colorError` when >0; last-success / last-failure as two separate rows, failure in error colour | ⚠️ Partial. Tiles landed but **inside the hero card**, as bare columns with **no tile fill**; the `activity_heading` string exists but is unused. `statsFailedText` is **never colour-set** — 5 failures read identically to 0. Last success + last failure are recombined into one `statsLastText` via `StringBuilder` (the §1 concatenation pattern), single `onSurfaceVariant` colour — the failure line is not error-coloured. |
| §2.5 Empty stats: tiles show "—" before first forward | ❌ Dropped. Tiles always render `String.valueOf(count)` — a fresh install shows `0/0/0`. (`no_forwards_yet` *did* land for the last-activity line.) |
| §2.5 First-run setup checklist card (Connect → Grant → Test) | ❌ Dropped entirely — no checklist view/logic. The not-configured state is headline + config text only. This was §2.5's headline onboarding item; dropping it is the review's biggest product gap after F2, though the banner system partially covers steps 1–2. |
| §2.3B Settings grouping ("Server" / "Filtering" / "This device" headings) | ❌ Dropped, except a `TitleSmall` "Device ID" heading. Cosmetic; **acceptable** on a 4-field form. |
| §2.3B Sticky bottom bar: Save filled (single primary) + Test outlined + "tests last saved settings" note | ✅ Landed. Order in the bar is Test (left) then Save (right) — primary rightmost is the conventional placement; **acceptable** difference from the spec's listing order. |
| §2.3B Field details: URL helper "Starts with https://", key password-toggle, senders helper + `minLines=3`, merchant "(optional)" | ✅ All landed verbatim (`server_url_helper`, `endIconMode=password_toggle`, `allowed_senders_helper`, `merchant_label_label` carries "(optional)" in the label string rather than as a suffix — equivalent). |
| §2.3C Notification brand icon (`ic_stat_miraclepay`) | Out of scoped files (`Notify.java` not in this review's scope) — not assessed. |
| Dynamic colour | Correctly **not** adopted — §2.1 said static brand first. ✅ |

---

## 2. Compile-risk spots (spotted by reading only)

Ranked by severity if wrong. None of these was build-verified in this review.

- **R1 — `Widget.Material3.Button.IconButton` on `copyDeviceIdButton` (activity_settings.xml).**
  The copy control is a `MaterialButton` styled `@style/Widget.Material3.Button.IconButton`
  carrying only `app:icon` + `contentDescription`, no text. If that style name does not
  resolve in `material:1.12.0`, this is a hard AAPT link failure — the single most
  dangerous line in the scoped layouts. (The M3 icon-button styles were added to the
  Material library in the 1.10 era, so it is *expected* to resolve — but it is the one
  reference in the redesign that should be confirmed by the CI assemble log, not assumed.)
  Even if it links, a text-less `MaterialButton` can render with button min-width/padding
  rather than as a compact 48dp icon target — worth a screenshot check.
- **R2 — Java field types are the *parents* of the XML widget classes (works, fragile).**
  `MainActivity.enableSwitch` is declared `SwitchCompat` while the layout inflates
  `MaterialSwitch`; `settingsButton/testButton/permissionButton/batteryButton/retryButton`
  are declared `android.widget.Button` while the layouts inflate `MaterialButton`;
  `SettingsActivity` binds `MaterialToolbar` into an `androidx.appcompat.widget.Toolbar`
  local and `copyDeviceIdButton` into `Button`. All compile and run **only because**
  `MaterialSwitch extends SwitchCompat`, `MaterialButton extends AppCompatButton`, and
  `MaterialToolbar extends Toolbar` in 1.12.0, and `findViewById`'s unchecked generic
  cast succeeds against the subclass at runtime. Safe today; it will mislead the next
  editor (e.g. calling a `MaterialSwitch`-only API will not compile) and breaks loudly
  if any layout widget is ever swapped for a sibling class. One-line-per-field fix.
- **R3 — `values-night/themes.xml` duplicates the entire light theme.**
  Legal and currently consistent (only `windowLightStatusBar` differs, correctly), but
  it is a drift trap: any future edit to `values/themes.xml` that is not mirrored will
  silently not apply at night. The duplication is also unnecessary for colour roles —
  `values/themes.xml` already references `@color/md_*`, which resolve to the night
  palette automatically. Only the `windowLightStatusBar` item needs the night file.
- **R4 — `RecyclerView` with `layout_height="wrap_content"` + `nestedScrollingEnabled="false"` inside a `ScrollView` (activity_main.xml).**
  Compiles and works, but wrap-content measurement defeats view recycling — every row
  is created and bound on each `refreshStatus()`. Fine for a handful of stuck SMS;
  a large backlog (hundreds of entries after days offline) will jank `onResume`.
  Performance risk, not correctness.
- **R5 — Snackbar anchoring.** Both activities call
  `Snackbar.make(findViewById(android.R.id.content), …)`. With no `CoordinatorLayout`
  in either hierarchy the Snackbar cannot be swipe-dismissed or dodge a FAB, and in
  Settings it will overlay the sticky action bar while shown. Functional, matches the
  letter of §3.7, below the spec's implied polish. Not a compile risk.
- **R6 — Minor reading nits (no compile impact).**
  `OutboxAdapter` upper-cases the monogram with the no-arg `toUpperCase()` (default
  locale — the Turkish-`i` class of bug; use `Locale.ROOT`/uppercase char);
  `MainActivity.retryOutbox()` increments `requeued` for *every* entry including
  already-pending ones and re-enqueues them (pre-existing logic, surfaced here only
  because the plurals toast now reports that count as "Retrying N messages");
  `SettingsActivity.testConnection()` javadoc still promises a "toast";
  `android:singleLine="true"` (deprecated in favour of `maxLines`) on three settings
  fields — compiles, lint noise only. Theme-attribute colours inside the dot/avatar
  shape drawables (`android:color="?attr/…"`) are fine at `minSdk 23`.

**Closed risks (checked, not risks):** all `R.id` references in the three Java files
exist in the layouts; all `@string/@dimen/@drawable` references in the layouts exist;
`OutboxStore.Entry` exposes every field the adapter reads (`sender`, `body`,
`receivedAtMillis`, `simSlot`, `status`); `StatsStore` exposes every getter
`MainActivity` calls; `app:helperText` / `app:endIconMode` are valid
`TextInputLayout` attributes in 1.12.0.

---

## 3. Ranked remaining fixes

1. **F1 — Confirm R1 in CI, then fix the field-type mismatches (R2).** Verify
   `Widget.Material3.Button.IconButton` links under `material:1.12.0` in the assemble
   log; redeclare `enableSwitch` as `MaterialSwitch`, buttons as `MaterialButton`,
   toolbar as `MaterialToolbar`. Minutes of work, removes the only plausible
   build-breaker and a standing trap.
2. **F2 — Implement the §2.4 matrix faithfully.** Gate banners: not-configured →
   show a new not-configured banner (→ Open settings) and suppress the other two;
   permission banner only when configured; battery banner only when
   configured + granted. Cap at two banners. Add the hero-card state tint
   (`primaryContainer` wash healthy / `errorContainer` wash blocked) — it is the
   matrix column users actually see first.
3. **F3 — Finish the queue rows.** Give `statusText` a real chip treatment
   (container background: `secondaryContainer` waiting / `errorContainer` failed),
   surface `Entry.lastError` under failed rows, add the header count chip, and
   promote the empty state to title (`queue_clear_title`, already in strings) +
   body. The data and strings already exist; this is layout + ~10 lines.
4. **F4 — Make failures read as failures in Activity.** Colour `statsFailedText`
   with `colorError` when >0 (and the last-failure line likewise — split
   `statsLastText` into the two specced rows), give the tiles their
   `surfaceContainer` fill / "Activity" heading, and show "—" instead of `0`
   before the first forward.
5. **F5 — Bring Settings' test connection to Main's standard** (progress
   indicator, inline result, Snackbar Retry) and split Main's
   `configSummaryText` into Server / Device-ID rows with the specced Copy
   action. Both are copy-the-pattern jobs from code already in the tree.
6. **F6 — String hygiene sweep.** Move `"Retry"`, `"SMS permission granted."`,
   the battery-fallback sentence, and `"SIM "` into `strings.xml`; fix the stale
   "toast" javadoc in `SettingsActivity`; delete or use the orphaned strings
   (`queue_clear_title`, `activity_heading` — both resolved by F3/F4).
7. **F7 — First-run checklist (§2.5).** Largest remaining item; schedule as its
   own wave. Until then, F2's not-configured banner is the minimum viable
   onboarding.
8. **F8 — Housekeeping.** Collapse `values-night/themes.xml` to the single
   `windowLightStatusBar` override (R3); add `colorOnTertiaryContainer` tokens
   for completeness; consider a default `MaterialCardView` style so the
   `surfaceContainerLow` fill is a token, not a theme default (1c).

**Overall:** Tier 1 ≈ 85% landed (tokens/theme/type essentially perfect),
Tier 2 ≈ 70% (feedback modernised; state matrix simplified), Tier 3 ≈ 80%
(list real and useful; chip/reason/count details missing). No dropped item
requires an architecture change to restore — every fix above fits §3's
"small, localised edits" envelope.

*Evidence base: the scoped files listed in the header, read in full at
review time (Wave 195). No source files were modified by this review.*
