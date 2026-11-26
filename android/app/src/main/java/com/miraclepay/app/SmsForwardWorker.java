// MiraclePay v0.0.1 — SmsForwardWorker.java
// Clean-room implementation. Original code written for MiraclePay;
// no part of this file is copied from any other project.
package com.miraclepay.app;

import android.content.Context;
import android.util.Log;

import androidx.annotation.NonNull;
import androidx.work.Data;
import androidx.work.Worker;
import androidx.work.WorkerParameters;

/**
 * Background worker that forwards one received SMS to the MiraclePay
 * backend.
 *
 * <p>{@code SmsReceiver} (which must finish quickly and cannot do
 * network I/O on the broadcast thread) enqueues this worker with the
 * raw message in its input {@link Data}; this class then performs the
 * actual HTTP POST off the main thread, where WorkManager can run it
 * under its normal constraints, backoff and retry policy.
 *
 * <h2>Input data</h2>
 * <ul>
 *   <li>{@link #KEY_SENDER} ({@code "sender"}) — String: the message's
 *       originating address / alphanumeric sender ID, exactly as
 *       received.</li>
 *   <li>{@link #KEY_BODY} ({@code "body"}) — String: the full message
 *       body, exactly as received.</li>
 *   <li>{@link #KEY_RECEIVED_AT} ({@code "received_at"}) — long: when
 *       the message was received on this device, as epoch
 *       milliseconds.</li>
 *   <li>{@link #KEY_SIM_SLOT} ({@code "sim_slot"}) — int: zero-based
 *       SIM slot the message arrived on, or {@code -1} when the device
 *       does not report one (single-SIM devices, unknown slot).</li>
 * </ul>
 * These key names are identical to the ones {@code SmsReceiver}
 * declares and writes, so the receiver/worker hand-off matches.
 *
 * <h2>Result policy</h2>
 * <ul>
 *   <li>Not configured (no usable backend URL / device key saved), or
 *       a missing/blank sender or body: {@link Result#failure()} —
 *       retrying the same work cannot fix either.</li>
 *   <li>Backend accepted the SMS (2xx, {@link ApiClient.Result#success}):
 *       {@link Result#success()}.</li>
 *   <li>Backend rejected it with a 4xx client error (for example 400
 *       for a malformed payload, or 401 for a device key the server
 *       does not accept): {@link Result#failure()}. Retrying would
 *       resend the identical request and fail the same way. A 401 may
 *       become fixable later if the user corrects the key in
 *       settings, but that is a new configuration, not a reason to
 *       keep retrying this request in a loop.</li>
 *   <li>Backend failed with a 5xx server error, any other unexpected
 *       status, or no status at all ({@link ApiClient#NO_STATUS},
 *       i.e. a network error — no connectivity, DNS failure, timeout):
 *       {@link Result#retry()} — these are transient by nature and
 *       WorkManager will re-run this worker with backoff.</li>
 * </ul>
 *
 * <p>The message body is never written to the log: payment SMS carry
 * amounts, transaction ids and account details.
 */
public class SmsForwardWorker extends Worker {

    /** Log tag for every message emitted by this worker. */
    public static final String TAG = "MiraclePaySms";

    /** Input-data key: SMS sender (String). Same value as in SmsReceiver. */
    public static final String KEY_SENDER = "sender";

    /** Input-data key: SMS body (String). Same value as in SmsReceiver. */
    public static final String KEY_BODY = "body";

    /** Input-data key: reception time, epoch milliseconds (long). */
    public static final String KEY_RECEIVED_AT = "received_at";

    /** Input-data key: zero-based SIM slot, or -1 if unknown (int). */
    public static final String KEY_SIM_SLOT = "sim_slot";

    /** SIM-slot value used when the input data carries none. */
    public static final int SIM_SLOT_UNKNOWN = -1;

    public SmsForwardWorker(@NonNull Context context,
                            @NonNull WorkerParameters workerParams) {
        super(context, workerParams);
    }

    @NonNull
    @Override
    public Result doWork() {
        // Android 14 reliability (Wave 42): run the forward as a
        // foreground (dataSync) execution so the OS does not kill
        // the process mid-request on aggressive OEM builds. The
        // manifest declares the dataSync type for WorkManager's
        // SystemForegroundService and the FOREGROUND_SERVICE /
        // FOREGROUND_SERVICE_DATA_SYNC permissions. If promotion
        // fails (e.g. started from a restricted background state),
        // fall back to plain background execution — WorkManager
        // retries still apply.
        try {
            setForegroundAsync(new androidx.work.ForegroundInfo(
                    Notify.FOREGROUND_NOTIFICATION_ID,
                    Notify.foregroundNotification(getApplicationContext()),
                    android.content.pm.ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC));
        } catch (Exception e) {
            Log.w(TAG, "Foreground promotion unavailable; running in background", e);
        }

        Data input = getInputData();
        String sender = input.getString(KEY_SENDER);
        String body = input.getString(KEY_BODY);
        long receivedAt = input.getLong(KEY_RECEIVED_AT, 0L);
        int simSlot = input.getInt(KEY_SIM_SLOT, SIM_SLOT_UNKNOWN);

        // A forward with no sender or no body is not a real SMS and
        // the backend can only reject it; fail permanently instead of
        // burning retries on it.
        if (sender == null || sender.trim().isEmpty()
                || body == null || body.trim().isEmpty()) {
            Log.w(TAG, "Dropping SMS forward with missing sender or body");
            return Result.failure();
        }

        // If the receiver did not stamp a reception time, fall back to
        // "now" rather than forwarding the epoch: the server uses this
        // value to order and deduplicate messages.
        if (receivedAt <= 0L) {
            receivedAt = System.currentTimeMillis();
        }

        ConfigStore config = new ConfigStore(getApplicationContext());
        if (!config.isConfigured()) {
            // The user has not finished setup (no backend URL and/or
            // no device key). There is nothing to forward to, and
            // retrying will not configure the app, so fail; messages
            // received after setup is completed are enqueued afresh
            // by the receiver.
            Log.w(TAG, "Not configured; cannot forward SMS from " + sender);
            return Result.failure();
        }

        // ApiClient.postSms never throws: network failures come back
        // as a failed Result with status ApiClient.NO_STATUS.
        ApiClient.Result result =
                ApiClient.postSms(config, sender, body, receivedAt, simSlot);
        int status = result.statusCode;

        if (result.success) {
            Log.i(TAG, "Forwarded SMS from " + sender
                    + " (HTTP " + status + ")");
            return Result.success();
        }

        if (status >= 400 && status < 500) {
            // Client error: the request itself is unacceptable
            // (malformed payload, unknown/revoked device key, missing
            // scope, ...). Resending it unchanged would fail
            // identically, so do not retry.
            Log.w(TAG, "Backend rejected SMS from " + sender
                    + " with client error HTTP " + status
                    + ": " + result.errorMessage + "; not retrying");
            return Result.failure();
        }

        // Everything else is transient: a 5xx server error, an
        // unexpected status (e.g. a stray 3xx from a proxy in front of
        // the backend), or NO_STATUS, meaning the request never got an
        // answer at all because of a network error. Let WorkManager
        // retry with backoff.
        if (status == ApiClient.NO_STATUS) {
            Log.w(TAG, "Network error forwarding SMS from " + sender
                    + ": " + result.errorMessage + "; will retry");
        } else {
            Log.w(TAG, "Backend returned HTTP " + status
                    + " for SMS from " + sender
                    + ": " + result.errorMessage + "; will retry");
        }
        return Result.retry();
    }
}
