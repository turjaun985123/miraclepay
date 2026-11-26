// MiraclePay v0.0.1 — SmsReceiver.java
// Clean-room implementation. Original code written for MiraclePay;
// no part of any third-party SMS forwarder was copied or consulted.
package com.miraclepay.app;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.provider.Telephony;
import android.telephony.SmsMessage;
import android.util.Log;


import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Receives incoming SMS broadcasts and hands each message off to
 * {@link SmsForwardWorker} for forwarding to the MiraclePay backend.
 *
 * <p>This receiver deliberately does as little as possible: it never
 * touches the network, a database or the filesystem. Its only jobs are
 * to reassemble multi-part messages, check that forwarding is switched
 * on and configured, and enqueue durable WorkManager work. WorkManager
 * then owns retries, backoff and surviving process death, so a message
 * is not lost just because the phone is offline or this process is
 * killed as soon as {@link #onReceive} returns.
 *
 * <p>Declared in the manifest with the {@code BROADCAST_SMS} permission
 * and a high-priority intent filter for
 * {@code android.provider.Telephony.SMS_RECEIVED}.
 */
public final class SmsReceiver extends BroadcastReceiver {

    private static final String TAG = "SmsReceiver";

    /** WorkManager tag applied to every SMS forwarding request. */
    public static final String TAG_SMS_FORWARD = "sms_forward";

    // Input Data keys consumed by SmsForwardWorker. The worker reads
    // exactly these names, so they live here as the single source of
    // truth for the receiver/worker hand-off.
    public static final String KEY_SENDER = "sender";
    public static final String KEY_BODY = "body";
    public static final String KEY_RECEIVED_AT = "received_at";
    public static final String KEY_SIM_SLOT = "sim_slot";

    /**
     * Intent extra some telephony stacks include to identify which SIM
     * a message arrived on. Not part of the platform contract, so it is
     * read defensively and defaults to {@link #SIM_SLOT_UNKNOWN}.
     */
    private static final String EXTRA_SLOT = "slot";

    /** Value used for {@link #KEY_SIM_SLOT} when the slot is unknown. */
    public static final int SIM_SLOT_UNKNOWN = -1;

    /** Initial exponential backoff before a failed forward is retried. */
    private static final long BACKOFF_SECONDS = 30L;

    /** Mutable accumulator used while joining multi-part messages. */
    private static final class MessageGroup {
        final StringBuilder body = new StringBuilder();
        /** Timestamp (epoch millis) of the first part seen. */
        final long receivedAtMillis;

        MessageGroup(long receivedAtMillis) {
            this.receivedAtMillis = receivedAtMillis;
        }
    }

    @Override
    public void onReceive(Context context, Intent intent) {
        if (context == null || intent == null) {
            return;
        }
        if (!Telephony.Sms.Intents.SMS_RECEIVED_ACTION.equals(intent.getAction())) {
            return;
        }

        SmsMessage[] parts = Telephony.Sms.Intents.getMessagesFromIntent(intent);
        if (parts == null || parts.length == 0) {
            Log.w(TAG, "SMS broadcast carried no message parts; ignoring");
            return;
        }

        // A long (multi-part) SMS is delivered as several SmsMessage
        // parts. Group by originating address and concatenate the part
        // bodies in delivery order so the worker forwards one complete
        // message per sender, stamped with the first part's time.
        Map<String, MessageGroup> bySender = new LinkedHashMap<>();
        for (SmsMessage part : parts) {
            if (part == null) {
                continue;
            }
            String sender = part.getOriginatingAddress();
            if (sender == null) {
                sender = "";
            }
            MessageGroup group = bySender.get(sender);
            if (group == null) {
                group = new MessageGroup(part.getTimestampMillis());
                bySender.put(sender, group);
            }
            String body = part.getMessageBody();
            if (body != null) {
                group.body.append(body);
            }
        }
        if (bySender.isEmpty()) {
            return;
        }

        // Forwarding is opt-in: stay completely silent (apart from a log
        // line) until the user has both configured a backend and turned
        // forwarding on. Checking here keeps disabled installs from
        // accumulating pointless work in the WorkManager database.
        ConfigStore config = new ConfigStore(context);
        if (!config.isEnabled()) {
            Log.i(TAG, "SMS received but forwarding is disabled; skipping "
                    + bySender.size() + " message(s)");
            return;
        }
        if (!config.isConfigured()) {
            Log.i(TAG, "SMS received but MiraclePay is not configured; skipping "
                    + bySender.size() + " message(s)");
            return;
        }

        int simSlot = intent.getIntExtra(EXTRA_SLOT, SIM_SLOT_UNKNOWN);

        // Wave 43: record each message in the visible outbox
        // first, then enqueue through the shared Forwarder so the
        // worker can report the outcome back onto the entry.
        OutboxStore outbox = new OutboxStore(context);
        for (Map.Entry<String, MessageGroup> entry : bySender.entrySet()) {
            String body = entry.getValue().body.toString();
            String outboxId = outbox.add(
                    entry.getKey(), body, entry.getValue().receivedAtMillis, simSlot);
            Forwarder.enqueue(context, outboxId, entry.getKey(), body,
                    entry.getValue().receivedAtMillis, simSlot);
            Log.d(TAG, "Queued SMS forward for sender " + entry.getKey());
        }
    }
}
