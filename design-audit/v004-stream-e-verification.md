# MiraclePay v0.0.4 — Stream E (Android Bangla localization) — Verification Report

- **Seat:** E4 (Verifier), Stream E — read-only verification; no source files modified by this seat.
- **Repo:** `~/workspace/miraclepay`, branch `main`, HEAD `bed6b4e` (Stream E work is uncommitted in the shared working tree).
- **Final verification snapshot:** 2026-11-28 14:55 UTC (files re-checked twice, ~1 min apart, md5-stable:
  `values/strings.xml` = `74b0fa33202707365fa72e2ea30669a5`, `values-bn/strings.xml` = `7d92cac5373586e9a383356b46157e63`).
- **Timeline note:** at first check (~14:35 UTC) none of Stream E had landed. E3 landed first
  (2 layout hints), then E1 (`values-bn/strings.xml`, 14:53 UTC), then E2 (Java resource-ification,
  expanding `values/strings.xml` from 54 → 66 strings). All counts below are from the **final** snapshot.
  The 12-string Bangla coverage gap in §2 is a direct consequence of that ordering: E1 translated the
  54-string base; E2 afterwards added 12 new English strings that E1 has not yet translated.

**Method:** a Python 3 script parsed both `strings.xml` files with `xml.etree.ElementTree`, extracted
format specifiers with the regex `%(?:(\d+)\$)?[-#+ 0,(<]*\d*(?:\.\d+)?([a-zA-Z%])` (set **and** order
compared as an ordered list), diffed the name sets (strings + plurals separately), XML-parsed every
resource file + `AndroidManifest.xml`, raw-scanned both strings files for bare `&` / unescaped
apostrophes / bare `<`, and cross-checked every `R.string.*` / `R.plurals.*` reference in Java and every
`@string/*` reference in layouts/manifest against `values/strings.xml`. Java/layout literals were
grep-extracted and hand-judged user-facing vs internal.

---

## 1. Format-specifier parity — ✅ PASS (0 violations)

- Strings present in **both** files: **54 / 54** compared — specifier set and order match exactly for all.
- Plurals present in both: **1 / 1** (`retrying_messages`): EN `one`/`other` = `%d`; BN `one`/`other` = `%d` — match.
- Caveat: parity could not be checked for the 12 English-only strings in §2 (no Bangla counterpart yet).
  Four of them are formatted and **will** be exercised through `getString(id, …)` in Bangla locale once
  translated, so their translations must preserve the specifiers exactly:
  - `connection_ok_http` — `%1$d` — called at `MainActivity.java:284`, `SettingsActivity.java:196`
  - `connection_failed_http` — `%1$d` — `MainActivity.java:290`, `SettingsActivity.java:202`
  - `connection_failed_reason` — `%1$s` — `MainActivity.java:287`, `SettingsActivity.java:199`
  - `server_device_summary` — `%1$s, %2$s, %3$s, %4$s` — `MainActivity.java:172`
  - `failure_http` — `%1$d, %2$s` — `SmsForwardWorker.java:186`; `failure_http_status` — `%1$d` — `SmsForwardWorker.java:203`

**Verdict: ship** (for the 54 shared strings; re-run parity after the §2 gap is closed).

## 2. Coverage — ⚠️ 54 / 66 strings (81.8%) + 1 / 1 plurals

Missing in `values-bn/strings.xml` (**12**) — all added by E2 *after* E1's translation pass; in Bangla
locale these fall back to English (no crash, but untranslated UI). All in
`android/app/src/main/res/values/strings.xml`:

| # | Name | values/strings.xml line | English value |
|---|------|------------------------|---------------|
| 1 | `retry` | :71 | `Retry` |
| 2 | `battery_unrestricted_hint` | :72 | `Open Settings → Battery and set MiraclePay to Unrestricted.` |
| 3 | `sms_permission_granted` | :73 | `SMS permission granted.` |
| 4 | `connection_ok_http` | :74 | `Connection OK (HTTP %1$d).` |
| 5 | `connection_failed_http` | :75 | `Connection failed (HTTP %1$d).` |
| 6 | `connection_failed_reason` | :76 | `Connection failed: %1$s` |
| 7 | `server_device_summary` | :77 | `%1$s: %2$s\n%3$s: %4$s` |
| 8 | `failure_missing_sender_or_body` | :78 | `missing sender or body` |
| 9 | `failure_app_not_configured` | :79 | `app not configured` |
| 10 | `failure_http` | :80 | `HTTP %1$d: %2$s` |
| 11 | `failure_http_status` | :81 | `HTTP %1$d` |
| 12 | `failure_network_error` | :82 | `network error` |

- Dead entries in `values-bn` (no `values` counterpart): **0** strings, **0** plurals.
- Observation (pre-existing, not a Stream E regression): `queue_clear_title` and `activity_heading`
  exist in both files but are referenced by no Java or layout code.

**Verdict: ship-with-fixes** — E1 to add the 12 Bangla translations above (preserving the §1 specifiers),
then re-run the parity script.

## 3. XML validity & aapt-sensitive issues — ✅ PASS

- XML parse: **19 / 19 files OK** — both strings files, all 3 layouts
  (`activity_main.xml`, `activity_settings.xml`, `item_outbox.xml`), all drawables, `values/*`,
  `values-night/*`, `values-bn/strings.xml`, and `AndroidManifest.xml`. 0 parse failures.
- aapt-sensitive raw scan of both strings files: **0** bare `&`, **0** bare `<` in values,
  **0** unescaped apostrophes. (The English source correctly uses `can\'t` in
  `banner_permission_body` / `queue_clear_body`; the Bangla translations contain no apostrophes.)
- Note: no local Android build was run (verification is static, per the read-only seat); aapt2 itself
  was not executed.

**Verdict: ship.**

## 4. Residual hard-coded user-facing literals — ⚠️ 5 direct + 5 indirect

E2/E3 results first: `MainActivity.java`, `SettingsActivity.java`, `SmsForwardWorker.java` are clean of
direct UI literals (the former `"Retry"`, `"SMS permission granted."`,
`"Open Settings → Battery …"` and the connection-result concatenations are now resources);
layouts contain **0** hard-coded `android:text` / `android:hint` / `android:contentDescription` /
`app:title|subtitle|helperText` values (13 `tools:text="…"` attributes remain — preview-only, never
shown at runtime, correctly out of scope).

Remaining **direct** user-facing literals — all in `Notify.java` (not touched by E2):

| # | Location | Literal | Why user-facing |
|---|----------|---------|-----------------|
| 1 | `Notify.java:45` | `"SMS forwarding"` | Notification-channel name, shown in system Settings → Notifications |
| 2 | `Notify.java:47` | `"Shown while MiraclePay forwards a payment SMS."` | Channel description, shown in system Settings |
| 3 | `Notify.java:56` | `"Forwarding payment SMS…"` | Foreground-notification body text, shown in the shade while forwarding |
| — | `Notify.java:55` | `"MiraclePay"` | Notification title; brand name (= `app_name`, untranslated in Bangla too) — acceptable, listed for completeness, not counted |

Remaining **layout/manifest** literal:

| # | Location | Literal | Note |
|---|----------|---------|------|
| 4 | `AndroidManifest.xml:40` | `android:label="MiraclePay"` | Should be `@string/app_name`; impact nil today (brand name identical in Bangla) — low |

Remaining **minor**:

| # | Location | Literal | Note |
|---|----------|---------|------|
| 5 | `OutboxAdapter.java:60` | `"SIM " + entry.simSlot` (×2 on the line) | Queue-row meta text; "SIM" is a universal abbreviation — low |

**Indirect** user-facing literals (5 distinct strings, 9 call sites) — `ApiClient.java` has no
`Context`, so E2 could not resource-ify it; its error strings are displayed verbatim to the user via
`getString(R.string.connection_failed_reason, result.getErrorMessage())`
(`MainActivity.java:287`, `SettingsActivity.java:199`), so in Bangla locale the failure reason stays English:

- `ApiClient.java:149` and `:215` — `"No configuration available"`
- `ApiClient.java:153` and `:219` — `"Server URL is not configured"`
- `ApiClient.java:166` — `"Could not build request: "`
- `ApiClient.java:192` and `:228` — `"Network error: "`
- `ApiClient.java:268–269` — `"Server returned HTTP " + status [+ ": " + responseBody]`

Correctly **left alone** (internal, not counted): all `Log.*` messages, JSON keys, HTTP
methods/headers, SharedPreferences keys, WorkManager keys/tags, `Result{…}` `toString()`, and
exception messages (e.g. `ConfigStore.java:50`). `SmsForwardWorker` failure-reason strings that
surface as the "Last failure" reason were properly moved to resources by E2 (`failure_*`, §2) —
only their Bangla translations are outstanding.

**Count:** **5 must-fix direct literals** (3× `Notify.java` :45/:47/:56, manifest label,
`OutboxAdapter` SIM line; the `Notify.java:55` brand title is excluded) and **5 indirect**
`ApiClient` strings (9 call sites).

**Verdict: ship-with-fixes** — resource-ify `Notify.java` (needs 3 new strings + Bangla translations)
and the manifest label; `ApiClient` needs a design call (pass localized strings in, or map error
codes to resources at the display site).

## 5. Resource integrity — ✅ PASS (0 dangling references)

- Java → `values/strings.xml`: **33 distinct** `R.string.*` names + **1** `R.plurals.*`
  (`retrying_messages`, `MainActivity.java:264`) referenced; **all 34 exist** in `values/strings.xml`. 0 missing.
- Layouts + manifest → `values/strings.xml`: **33 distinct** `@string/*` names referenced across
  `activity_main.xml`, `activity_settings.xml` (+ none in `item_outbox.xml` / manifest); **all exist**. 0 missing.
- E3's two new resources resolve: `@string/settings_server_url_hint` (`activity_settings.xml:48`),
  `@string/settings_device_key_hint` (`activity_settings.xml:68`).

**Verdict: ship.**

---

## Summary

| # | Check | Result | Verdict |
|---|-------|--------|---------|
| 1 | Specifier parity | **0 violations** / 54 shared strings + 1 plural | ship |
| 2 | Coverage | **54 / 66** strings (81.8%), 1/1 plurals, 0 dead BN entries; 12 E2-added strings untranslated | ship-with-fixes |
| 3 | XML validity / aapt | **19/19** files parse; 0 bare `&`/`<`, 0 unescaped apostrophes | ship |
| 4 | Residual hard-coding | **5 direct** must-fix literals (`Notify.java:45,47,56`; `AndroidManifest.xml:40`; `OutboxAdapter.java:60`) + **5 indirect** (`ApiClient.java` error strings, 9 sites); layouts 0, main activities 0 | ship-with-fixes |
| 5 | Resource integrity | **0** dangling refs (34 Java + 33 layout names, all resolve) | ship |

**Overall verdict: ship-with-fixes.** No crash-class defects: no specifier mismatches, no broken XML,
no dangling resources. Two fix batches before the Bangla locale is complete: (a) E1 adds the 12 missing
Bangla translations in §2 with §1 specifiers preserved, then parity is re-run; (b) E2 resource-ifies
`Notify.java` (and ideally the manifest label / `ApiClient` error surface). Nothing here blocks merging
Stream E's current state, since every gap degrades to English rather than crashing.
