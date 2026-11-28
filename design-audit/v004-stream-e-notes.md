# MiraclePay v0.0.4 — Stream E (Android Bangla / values-bn) — Seat E5: Docs/CI analysis

Read-only audit, 2026-11-28. No code changed by this seat. Line numbers are against the
working tree as read (sibling Stream E seats were landing `values-bn` concurrently —
see §5 snapshot note).

## 1. Android CI job — exactly what runs

File: `.github/workflows/ci.yml`, job `android` ("Android (assembleDebug)"), lines 194–248.
Trigger: every `push` and `pull_request` (workflow header). Runs on `ubuntu-latest`,
`working-directory: android`.

| Step | Lines | Detail |
|---|---|---|
| Checkout | 198–199 | `actions/checkout@v4` |
| JDK | 203–208 | `actions/setup-java@v4`, distribution `temurin`, **Java 17** (only version pinned) |
| Gradle | 210–215 | `gradle/actions/setup-gradle@v4`, **no `gradle-version` argument and no wrapper committed** (verified: no `gradlew`, no `android/gradle/wrapper/gradle-wrapper.properties` in the tree). The Gradle version is therefore **unpinned** — whatever setup-gradle v4 / the runner resolves. The only stated floor is a comment: "Requires Gradle 8.7+" (`android/build.gradle:7`, `android/settings.gradle:2`). |
| AGP / Kotlin | — | **AGP 8.5.2** pinned in the root plugins block (`android/build.gradle:10–12`); Kotlin plugin 2.0.20 declared there but not applied by the app module (pure Java). compileSdk/targetSdk 34 (see §2). The Android SDK itself is the runner image's preinstalled SDK — no `setup-android` step, no SDK version pinned in the workflow. |
| Build debug | 217–218 | `gradle assembleDebug` — the **only** verification task for the debug variant |
| Keystore decode | 220–233 | Only when `ANDROID_KEYSTORE_BASE64` secret exists; exports `ANDROID_KEYSTORE_PATH` |
| Build release | 235–241 | `gradle assembleRelease` (signed if the keystore env is present, unsigned otherwise) |
| Upload artifacts | 243–248 | `actions/upload-artifact@v4`, name `miraclepay-apks`, paths `android/app/build/outputs/apk/debug/*.apk` + `.../release/*.apk`, `if-no-files-found: error` |

**There is no other workflow file** (`.github/workflows/` contains only `ci.yml`) and no
release workflow — GitHub Releases are assembled from these CI artifacts.

**What CI does NOT run for Android:** no `lint` / `lintDebug` / `lintRelease`, no `check`,
no `build` lifecycle task (which is what would pull lint in), no unit tests
(`testDebugUnitTest` — and there is no `src/test` tree), no connected/instrumented tests,
no AAB (`bundleRelease` — CI ships plain universal APKs), and no post-build inspection of
the APK contents. `assemble*` depends on resource compile/link + Java compile + packaging
only; in AGP 8.x `assemble` does **not** depend on `lint`.

## 2. CI coverage table for Stream E's risks

| Stream E risk | Caught by CI (`assembleDebug`+`assembleRelease`)? | Why / why not |
|---|---|---|
| Malformed XML in `values-bn/strings.xml` (bad nesting, unescaped `'`/`&`, invalid escapes) | **YES — build error** | aapt2 compile/link runs inside assemble; XML/escape errors fail the link. |
| Duplicate string name inside one file, or a `<string>` in bn shadowing illegally | **YES — build error** | aapt2 duplicate-resource error at link time. |
| Code/layout referencing a string that does not exist in **default** `values/` (`R.string.*`, `@string/*`) | **YES — build error** | aapt2 link + javac both fail on the unresolved reference. This is the "missing-string" case that *is* a build error: the key must exist in the default set. |
| A default string **missing its Bangla translation** (key in `values/`, absent in `values-bn/`) | **NO** | Resource fallback to English is legal Android behaviour; aapt2 is silent. Only lint's `MissingTranslation` would flag it, and **lint never runs in CI** (see §1, §3). A partial bn file ships green and silently renders English for the missing keys. |
| A bn-only key with no default counterpart (`ExtraTranslation`) | **NO** (harmless at runtime) | aapt2 accepts extra locale resources; only lint would flag it. |
| **Format-specifier mismatch** bn vs default — count (`%d` dropped/duplicated), type (`%d`→`%s`), or positional order | **NO** | aapt2 does no cross-locale format comparison. Lint's `StringFormatMatches` / `StringFormatCount` would catch it, but lint is not invoked. Consequence is a **runtime** crash (`MissingFormatArgumentException` / `IllegalFormatConversionException`) or garbled text on a bn-locale device only — invisible to CI and to en-locale testing. This is Stream E's highest-severity blind spot. |
| Malformed format string within a single bn string (e.g. stray `%`, `%1$d` vs `%d` misuse) | **Partial** | aapt2 rejects some invalid format syntax at compile time; subtle but syntactically valid errors pass and fail only at runtime. Do not rely on assemble for this. |
| `<plurals>` quantity set wrong for Bangla (CLDR bn uses `one` + `other`, same pair as the default `retrying_messages`) | **NO** | Missing/extra quantities are lint `MissingQuantity`/`ImpliedQuantity` territory; aapt2 accepts the file and the runtime falls back to `other`. |
| Hardcoded user-visible text left in Java/layouts (never extracted, so never translated) | **NO** | `HardcodedText` is a lint check; lint does not run. Known pre-existing instances are listed in §3 — they stay English on a bn device regardless of how complete `values-bn` is. |
| bn resources stripped from the APK by config | **NO — but no stripping config exists** (see §3) | Nothing in the build filters locales; a universal APK from `assemble*` contains every `values-*` qualifier. CI would not *notice* stripping if a future change added it — hence the release-proof check in §6. |
| Translation quality / wrong wording / RTL or shaping problems | **NO** | Nothing automated can catch this; human read on a bn-locale device/emulator only. |
| App crashes only under bn locale (e.g. the specifier bug above) | **NO** | No instrumented tests, no locale-matrix run anywhere in CI. |

Bottom line, honestly: **CI proves the bn resources compile, link and package. It does not
prove they are complete, format-compatible, or even present in the shipped APK.** Completeness
and specifier parity are on the stream's own diff review (or a future `gradle lint` step —
a coordinator decision; adding one is a CI change, out of this seat's scope).

## 3. Gradle / lint findings (`android/app/build.gradle` unless noted)

- `android/app/build.gradle:14` — `minSdk 23`; `:15` — `targetSdk 34`; `:10` — `compileSdk 34`;
  `:9` — namespace `com.miraclepay.app`; `:16–17` — versionCode 6 / versionName "0.0.3.3".
  minSdk 23 has no bearing on `values-bn` (locale qualifiers work on every API level).
- **No `resConfigs` / `resourceConfigurations`** anywhere in `android/app/build.gradle`,
  `android/build.gradle`, or `android/settings.gradle` (grep-verified) — nothing restricts
  the packaged locales, so `bn` cannot be filtered out by the current config.
- **No `aaptOptions`** block, no `androidResources { localeFilters … }`, no `bundle { language {
  enableSplit = false } }` / splits config. Also not applicable in practice: CI builds
  universal APKs via `assemble*`, not an AAB, so Play-style language splits never occur.
- **No resource shrinking risk:** `android/app/build.gradle:38` — release `minifyEnabled false`,
  and there is no `shrinkResources` anywhere, so no locale resource can be shrunk away either.
- **No lint configuration at all:** no `lint { … }` / legacy `lintOptions { … }` block in any
  Gradle file, no `lint.xml` anywhere in the repo (find-verified), no lint baseline file.
  Consequences for this stream:
  - Nothing from Stream E can *fail* the build via lint — there is no `abortOnError` set and,
    more fundamentally, the lint task is never executed by the CI tasks above.
  - Equally, lint's defaults never protect the stream: `MissingTranslation` (error-severity
    in current AGP lint when lint *is* run), `StringFormatMatches`/`StringFormatCount`
    (warnings), `ExtraTranslation`, `MissingQuantity`, and `HardcodedText` are all dormant.
    If a future change adds `gradle lint` or switches CI to `gradle build`/`check`, expect
    `MissingTranslation`/`HardcodedText` findings on the pre-existing hardcoded strings below
    to surface immediately — they are not Stream E regressions.
- Pre-existing hardcoded user-visible strings that **will remain English under bn** unless
  E2/E3 extract them (CI will not flag any of them):
  - `MainActivity.java:305` — Snackbar action `"Retry"` (note: `SettingsActivity`'s identical
    flow uses string resources; only this one is literal).
  - `MainActivity.java:369` — `showSnackbar("Open Settings → Battery and set MiraclePay to Unrestricted.")` (battery-exemption fallback path).
  - `MainActivity.java:391` — `"SMS permission granted."` snackbar text.
  - `Notify.java:45` — notification channel name `"SMS forwarding"`; `:47` — channel
    description `"Shown while MiraclePay forwards a payment SMS."`; `:56` — foreground
    notification text `"Forwarding payment SMS…"` (title `"MiraclePay"` at :55 is the brand
    name, correctly untranslated). Caveat: a notification channel's name/description are
    frozen at channel creation, so even after extraction, existing installs keep the
    language the channel was created in until reinstall — worth knowing, not a blocker.
  - `ApiClient.java:149,153,166,192,215,219,228` — English error strings
    ("No configuration available", "Server URL is not configured", "Network error: …")
    that surface verbatim in the connection-test result line via `MainActivity` /
    `SettingsActivity`; they are data strings, not resources, so `values-bn` cannot reach them.
  - Layouts are otherwise clean: every `android:text`/`android:hint`/`android:contentDescription`
    in `res/layout/*.xml` is an `@string/` reference (grep-verified). The two literal hints
    that existed in `activity_settings.xml` (`https://pay.example.com`, `mp_live_...`) were
    extracted by E3 during this wave to `settings_server_url_hint` / `settings_device_key_hint`
    (now referenced at `activity_settings.xml:48,68`) — example values, correctly carried
    into bn untranslated.

## 4. Locale behaviour — how a Bangladeshi user actually gets the Bangla UI

**The app follows the system locale only. There is no in-app language selection, and none
was found anywhere:**

- `AndroidManifest.xml` — no `android:localeConfig`, no `android:configChanges` locale
  handling, no `AppLocales` service declaration (`androidx.appcompat.app.AppLocalesMetadataHolderService`),
  and no `res/xml/locales_config.xml` exists in the tree. `android:label="MiraclePay"`
  (:40) is the literal brand name, not `@string/app_name`. `android:supportsRtl="true"`
  (:42) is present; Bangla is LTR, so it has no effect on bn rendering.
- `MainActivity.java` / `SettingsActivity.java` — no `Locale` use, no `setLocale`,
  no `AppCompatDelegate.setApplicationLocales()`, no `attachBaseContext` /
  `createConfigurationContext` override (grep-verified across all Java sources). The only
  `Locale` references in the app are `Locale.US` for sender matching
  (`ConfigStore.java:209,211`) and a timestamp format (`ApiClient.java:316`) — neither is UI locale.
- `ConfigStore.java` — the complete key set is `server_url`, `device_key`, `device_id`,
  `merchant_label`, `enabled`, `allowed_senders` (lines 36–41). **No language/locale key
  exists**, and the Settings screen built on it has no language row.

**Resulting behaviour (documented, acceptable — do not build a toggle in this stream):**

1. Android's standard resource resolution does all the work: with `res/values-bn/strings.xml`
   present, a device whose locale is Bangla (`bn`, `bn-BD`, or `bn-IN` — the bare `bn`
   qualifier matches every bn region) renders the Bangla strings; every other locale falls
   back to the default English `values/`. No code change was needed for this to work, and
   AppCompat 1.7.0 (already a dependency) does not alter it without the per-app-locale APIs.
2. A Bangladeshi user gets the Bangla UI by setting their **phone's** language to বাংলা:
   Android Settings → System → Languages & input → Languages → add/move বাংলা (Bangla) to
   the top. On Samsung/Xiaomi BD handsets the same setting lives under General management /
   Additional settings → Language. The change applies to MiraclePay immediately on next
   launch (standard configuration change; the activities are recreated).
3. There is **no per-app shortcut to document**: the app declares no `localeConfig`, so it
   does not register its locales for Android 13+'s per-app language picker
   (Settings → Apps → MiraclePay → App language) — on Android 13/14 that list is driven by
   the declared locale config, so users should be told to use the system-wide language
   setting in step 2. (If a future release wants the per-app picker, it needs
   `android:localeConfig` + `locales_config.xml` — a named follow-up, not part of v0.0.4.)
4. Mixed-language reality to set expectations in release notes: any string missing from
   `values-bn` and any hardcoded string in §3 renders in English on a bn device — by
   design of the fallback, not by error.

## 5. Snapshot of the stream's own output at audit time

Sibling seats had already landed `android/app/src/main/res/values-bn/strings.xml` in the
shared tree when this audit read it. Verified mechanically at that moment:

- Key parity: the sorted `name="…"` sets of `values/strings.xml` and
  `values-bn/strings.xml` are **identical** (54 `<string>` entries + 1 `<plurals>` in the
  default file), including the two E3-extracted hint keys.
- Format parity for the only format-bearing entry: `retrying_messages` plurals carry
  exactly one `%d` in each of `one`/`other` in both locales — matching.
- This is a point-in-time check of a moving tree; the coordinator should re-run the parity
  check on the final tree (commands in §6) rather than trusting this snapshot.

## 6. What the release proof should check (for the coordinator)

CI green is **not** sufficient evidence for this stream (see §2). Before calling v0.0.4's
Bangla support shipped, verify on the actual release-candidate artifacts:

1. **Key + specifier parity on the final tree** (scriptable, no Android SDK needed):
   diff the sorted string/plural names of `values/strings.xml` vs `values-bn/strings.xml`
   (must be identical), and for every format-bearing key compare the ordered list of
   format specifiers (`%d`, `%s`, `%1$s`, …) between locales (must match exactly).
2. **bn resources are inside the built APK** — download the `miraclepay-apks` CI artifact
   (or the release APK attached to the GitHub Release) and check, in order of preference:
   - `apkanalyzer resources packages <apk>` / Android SDK cmdline-tools if available:
     `apkanalyzer resources value --config bn <apk> res/…` — heavyweight; the two checks
     below are sufficient and need no SDK.
   - **`aapt2 dump resources <apk> | grep -i "bn"`** (aapt2 ships with the SDK build-tools
     on any dev machine; it is *not* run in CI today) — expect a `values-bn` configuration
     block listing the string resources.
   - **Zero-dependency check with `unzip` alone** (available on this VM; aapt2/apkanalyzer
     are not installed here): `unzip -l <apk>` only proves `resources.arsc` exists — the
     locale table lives inside it, so follow with
     `unzip -p <apk> resources.arsc | strings | grep -c "সেটিংস"` (or another distinctive
     Bangla string such as `ফরওয়ার্ডিং চালু আছে`) — a non-zero count proves Bangla string
     values were compiled into the shipped APK, for **both** the debug and the release APK.
     Do this on the *release* APK specifically: it is the one users install, and it is the
     variant a future `resConfigs`/shrinking change would silently strip.
3. **Device/emulator smoke in bn**: install the release APK on an emulator or handset set
   to বাংলা (system language, per §4), walk Main + Settings screens, and specifically
   exercise the quantity-string path (Retry-all with ≥2 queued messages → `retrying_messages`)
   and the connection-test result line — the two places a specifier/format bug would crash
   or garble. Screenshot both screens for the release notes.
4. **Fallback sanity**: switch the same device back to English and confirm the UI is fully
   English again (guards against a bn string accidentally landing in the default file).
5. **Release-notes wording**: state plainly that Bangla follows the phone's system
   language and there is no in-app language switch (§4), so users on English-locale
   phones are not surprised.
