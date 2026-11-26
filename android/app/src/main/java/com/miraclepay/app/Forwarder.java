// MiraclePay v0.0.1 — Forwarder.java
// Clean-room implementation. Original code written for MiraclePay.
package com.miraclepay.app;

import android.content.Context;

import androidx.work.BackoffPolicy;
import androidx.work.Constraints;
import androidx.work.Data;
import androidx.work.NetworkType;
import androidx.work.OneTimeWorkRequest;
import androidx.work.WorkManager;

import java.util.concurrent.TimeUnit;

/**
 * The single place that turns a message into a WorkManager
 * forwarding request (Wave 43). Used by {@code SmsReceiver} for
 * new messages and by the main screen's "Retry now" button for
 * outbox entries, so both paths share constraints, backoff and
 * input keys.
 */
final class Forwarder {

    /** WorkManager tag applied to every SMS forwarding request. */
    static final String TAG_SMS_FORWARD = "sms_forward";

    /** Initial exponential backoff before a retry, in seconds. */
    private static final long BACKOFF_SECONDS = 30L;

    private Forwarder() {
    }

    /** Enqueue one forward. `outboxId` ties the work to its
     *  {@link OutboxStore} entry so the worker can update it. */
    static void enqueue(Context context, String outboxId, String sender, String body,
                        long receivedAtMillis, int simSlot) {
        Data input = new Data.Builder()
                .putString(SmsForwardWorker.KEY_SENDER, sender)
                .putString(SmsForwardWorker.KEY_BODY, body)
                .putLong(SmsForwardWorker.KEY_RECEIVED_AT, receivedAtMillis)
                .putInt(SmsForwardWorker.KEY_SIM_SLOT, simSlot)
                .putString(SmsForwardWorker.KEY_OUTBOX_ID, outboxId)
                .build();

        Constraints constraints = new Constraints.Builder()
                .setRequiredNetworkType(NetworkType.CONNECTED)
                .build();

        OneTimeWorkRequest request =
                new OneTimeWorkRequest.Builder(SmsForwardWorker.class)
                        .setInputData(input)
                        .setConstraints(constraints)
                        .setBackoffCriteria(
                                BackoffPolicy.EXPONENTIAL,
                                BACKOFF_SECONDS,
                                TimeUnit.SECONDS)
                        .addTag(TAG_SMS_FORWARD)
                        .build();

        WorkManager.getInstance(context).enqueue(request);
    }
}
