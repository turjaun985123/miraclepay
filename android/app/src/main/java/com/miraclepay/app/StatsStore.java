// MiraclePay v0.0.1 — StatsStore.java
// Clean-room implementation. Original code written for MiraclePay.
package com.miraclepay.app;

import android.content.Context;
import android.content.SharedPreferences;

/**
 * Lifetime forwarding statistics (Wave 110).
 *
 * <p>A tiny counter store in its own SharedPreferences file, kept
 * separate from {@link OutboxStore} on purpose: the outbox is a
 * bounded queue of messages still needing attention, while these
 * counters are cumulative history that survives entries being
 * delivered and dropped from the outbox.
 *
 * <p>Writers: {@code SmsForwardWorker} records a success for every
 * message the backend accepted and a failure for every permanent
 * (non-retried) failure; {@code SmsReceiver} records a filtered
 * count for every message skipped by the Wave 109 sender filter.
 * Transient failures that WorkManager will retry are not counted
 * as failures here — the eventual success or permanent failure is.
 *
 * <p>Reader: {@code MainActivity} shows the counters on the main
 * screen and refreshes them from its existing status-refresh path.
 */
public final class StatsStore {

    /** SharedPreferences file for the statistics. */
    public static final String PREFS_NAME = "miraclepay_stats";

    private static final String KEY_TOTAL_FORWARDED = "total_forwarded";
    private static final String KEY_TOTAL_FAILED = "total_failed";
    private static final String KEY_TOTAL_FILTERED = "total_filtered";
    private static final String KEY_LAST_SUCCESS_AT = "last_success_at";
    private static final String KEY_LAST_FAILURE_AT = "last_failure_at";
    private static final String KEY_LAST_FAILURE_REASON = "last_failure_reason";

    private final SharedPreferences prefs;

    public StatsStore(Context context) {
        this.prefs = context.getApplicationContext()
                .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
    }

    /** Record one message successfully forwarded to the backend. */
    public synchronized void recordSuccess() {
        prefs.edit()
                .putLong(KEY_TOTAL_FORWARDED, getTotalForwarded() + 1L)
                .putLong(KEY_LAST_SUCCESS_AT, System.currentTimeMillis())
                .apply();
    }

    /** Record one permanently failed forward, with its reason. */
    public synchronized void recordFailure(String reason) {
        prefs.edit()
                .putLong(KEY_TOTAL_FAILED, getTotalFailed() + 1L)
                .putLong(KEY_LAST_FAILURE_AT, System.currentTimeMillis())
                .putString(KEY_LAST_FAILURE_REASON, reason == null ? "" : reason)
                .apply();
    }

    /** Record one message skipped by the Wave 109 sender filter. */
    public synchronized void recordFiltered() {
        prefs.edit()
                .putLong(KEY_TOTAL_FILTERED, getTotalFiltered() + 1L)
                .apply();
    }

    /** Total messages successfully forwarded since install. */
    public synchronized long getTotalForwarded() {
        return prefs.getLong(KEY_TOTAL_FORWARDED, 0L);
    }

    /** Total messages that failed permanently since install. */
    public synchronized long getTotalFailed() {
        return prefs.getLong(KEY_TOTAL_FAILED, 0L);
    }

    /** Total messages skipped by the sender filter since install. */
    public synchronized long getTotalFiltered() {
        return prefs.getLong(KEY_TOTAL_FILTERED, 0L);
    }

    /** Epoch millis of the last successful forward, or 0 if none yet. */
    public synchronized long getLastSuccessAtMillis() {
        return prefs.getLong(KEY_LAST_SUCCESS_AT, 0L);
    }

    /** Epoch millis of the last permanent failure, or 0 if none yet. */
    public synchronized long getLastFailureAtMillis() {
        return prefs.getLong(KEY_LAST_FAILURE_AT, 0L);
    }

    /** Reason of the last permanent failure, or "" if none recorded. */
    public synchronized String getLastFailureReason() {
        String stored = prefs.getString(KEY_LAST_FAILURE_REASON, "");
        return stored == null ? "" : stored;
    }

    /** Reset every counter and timestamp back to zero/empty. */
    public synchronized void clear() {
        prefs.edit().clear().apply();
    }
}
