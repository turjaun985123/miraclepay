package com.miraclepay.app;

import android.content.Context;
import android.content.SharedPreferences;

import java.util.UUID;

/**
 * MiraclePay v0.0.1 — persistent configuration for the Android companion app.
 *
 * <p>Thin wrapper around a single {@link SharedPreferences} file. All values
 * the app needs to reach a MiraclePay server and identify itself live here:
 * the server base URL, the device API key issued from the dashboard, a
 * stable device identifier generated on first use, an optional human-readable
 * merchant label, and a master on/off switch for background work.</p>
 *
 * <p>Clean-room implementation. Plain Java; depends only on
 * {@link Context} and {@link SharedPreferences} from the Android framework.</p>
 */
public final class ConfigStore {

    /** Name of the underlying SharedPreferences file. */
    public static final String PREFS_NAME = "miraclepay_config";

    /** Placeholder base URL shipped as the default; not a real server. */
    public static final String DEFAULT_SERVER_URL = "https://your-server.example.com";

    private static final String KEY_SERVER_URL = "server_url";
    private static final String KEY_DEVICE_KEY = "device_key";
    private static final String KEY_DEVICE_ID = "device_id";
    private static final String KEY_MERCHANT_LABEL = "merchant_label";
    private static final String KEY_ENABLED = "enabled";

    private static final String DEVICE_ID_PREFIX = "android-";

    private final SharedPreferences prefs;

    /**
     * Creates a store backed by the application's SharedPreferences.
     *
     * @param context any context; the application context is used internally
     *                so no activity reference is retained.
     */
    public ConfigStore(Context context) {
        if (context == null) {
            throw new IllegalArgumentException("context must not be null");
        }
        this.prefs = context.getApplicationContext()
                .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
    }

    // ------------------------------------------------------------------
    // Server URL
    // ------------------------------------------------------------------

    /**
     * Returns the configured server base URL with any trailing slashes
     * removed, so callers can safely append paths such as {@code "/api/..."}.
     * Falls back to {@link #DEFAULT_SERVER_URL} when nothing is stored.
     */
    public String getServerUrl() {
        String stored = prefs.getString(KEY_SERVER_URL, DEFAULT_SERVER_URL);
        return trimTrailingSlashes(stored);
    }

    /** Stores the server base URL (trailing slashes are trimmed on read). */
    public void setServerUrl(String serverUrl) {
        prefs.edit()
                .putString(KEY_SERVER_URL, serverUrl == null ? "" : serverUrl.trim())
                .apply();
    }

    // ------------------------------------------------------------------
    // Device key (API key from the dashboard)
    // ------------------------------------------------------------------

    /** Returns the device API key, or an empty string when unset. */
    public String getDeviceKey() {
        String stored = prefs.getString(KEY_DEVICE_KEY, "");
        return stored == null ? "" : stored;
    }

    /** Stores the device API key issued from the MiraclePay dashboard. */
    public void setDeviceKey(String deviceKey) {
        prefs.edit()
                .putString(KEY_DEVICE_KEY, deviceKey == null ? "" : deviceKey.trim())
                .apply();
    }

    // ------------------------------------------------------------------
    // Device ID
    // ------------------------------------------------------------------

    /**
     * Returns this device's stable identifier. On the first call a new ID of
     * the form {@code "android-" + UUID} is generated and persisted; every
     * later call (including after process restarts) returns the same value.
     */
    public String getDeviceId() {
        String stored = prefs.getString(KEY_DEVICE_ID, null);
        if (stored != null && !stored.trim().isEmpty()) {
            return stored;
        }
        String generated = DEVICE_ID_PREFIX + UUID.randomUUID().toString();
        // commit() rather than apply(): the ID must survive an immediate
        // process death, otherwise a second ID could be minted.
        prefs.edit().putString(KEY_DEVICE_ID, generated).commit();
        return generated;
    }

    // ------------------------------------------------------------------
    // Merchant label
    // ------------------------------------------------------------------

    /** Returns the optional merchant label, or an empty string when unset. */
    public String getMerchantLabel() {
        String stored = prefs.getString(KEY_MERCHANT_LABEL, "");
        return stored == null ? "" : stored;
    }

    /** Stores a human-readable label identifying the merchant or shop. */
    public void setMerchantLabel(String merchantLabel) {
        prefs.edit()
                .putString(KEY_MERCHANT_LABEL,
                        merchantLabel == null ? "" : merchantLabel.trim())
                .apply();
    }

    // ------------------------------------------------------------------
    // Enabled flag
    // ------------------------------------------------------------------

    /** Returns whether the app's background features are enabled (default {@code true}). */
    public boolean isEnabled() {
        return prefs.getBoolean(KEY_ENABLED, true);
    }

    /** Enables or disables the app's background features. */
    public void setEnabled(boolean enabled) {
        prefs.edit().putBoolean(KEY_ENABLED, enabled).apply();
    }

    // ------------------------------------------------------------------
    // Bulk save helpers
    // ------------------------------------------------------------------

    /**
     * Saves the core connection settings in a single edit.
     *
     * @param serverUrl    server base URL (trailing slashes trimmed on read)
     * @param deviceKey    device API key from the dashboard
     * @param merchantLabel optional human-readable merchant label
     */
    public void save(String serverUrl, String deviceKey, String merchantLabel) {
        prefs.edit()
                .putString(KEY_SERVER_URL, serverUrl == null ? "" : serverUrl.trim())
                .putString(KEY_DEVICE_KEY, deviceKey == null ? "" : deviceKey.trim())
                .putString(KEY_MERCHANT_LABEL,
                        merchantLabel == null ? "" : merchantLabel.trim())
                .apply();
    }

    /**
     * Saves all editable settings, including the enabled flag, in a single edit.
     * The device ID is intentionally not part of this helper: it is generated
     * once by {@link #getDeviceId()} and never overwritten here.
     */
    public void save(String serverUrl, String deviceKey, String merchantLabel,
                     boolean enabled) {
        prefs.edit()
                .putString(KEY_SERVER_URL, serverUrl == null ? "" : serverUrl.trim())
                .putString(KEY_DEVICE_KEY, deviceKey == null ? "" : deviceKey.trim())
                .putString(KEY_MERCHANT_LABEL,
                        merchantLabel == null ? "" : merchantLabel.trim())
                .putBoolean(KEY_ENABLED, enabled)
                .apply();
    }

    /** Removes every stored value, returning the store to its defaults. */
    public void clear() {
        prefs.edit().clear().apply();
    }

    // ------------------------------------------------------------------
    // Readiness check
    // ------------------------------------------------------------------

    /**
     * Returns {@code true} when the app has enough configuration to talk to a
     * real server: a non-empty server URL that is not just the shipped
     * placeholder default, and a non-empty device key.
     */
    public boolean isConfigured() {
        String url = getServerUrl();
        if (url.isEmpty()) {
            return false;
        }
        if (url.equals(trimTrailingSlashes(DEFAULT_SERVER_URL))) {
            return false;
        }
        return !getDeviceKey().trim().isEmpty();
    }

    // ------------------------------------------------------------------
    // Internal helpers
    // ------------------------------------------------------------------

    /** Removes every trailing {@code '/'} character; {@code null} becomes "". */
    private static String trimTrailingSlashes(String value) {
        if (value == null) {
            return "";
        }
        String result = value.trim();
        while (result.endsWith("/")) {
            result = result.substring(0, result.length() - 1);
        }
        return result;
    }
}
