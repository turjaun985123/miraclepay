package com.miraclepay.app;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;
import android.util.Log;

/**
 * MiraclePay v0.0.1 — boot-completed receiver.
 *
 * <p>Original clean-room implementation for MiraclePay.</p>
 *
 * <p>This receiver intentionally does almost nothing, and that is by design:</p>
 *
 * <ul>
 *   <li>The SMS receiver ({@code SmsReceiver}) is <em>manifest-registered</em>,
 *       so Android activates it automatically as soon as the device finishes
 *       booting. There is no runtime registration to redo here, and no service
 *       needs to be started for incoming SMS messages to be received.</li>
 *   <li>Pending SMS forwards are scheduled through WorkManager. WorkManager
 *       persists its work queue in its own database, so any forward that was
 *       enqueued before a reboot or power loss is restored and run by
 *       WorkManager itself after boot — this receiver does not need to
 *       re-enqueue, rebuild, or replay anything.</li>
 * </ul>
 *
 * <p>All this receiver does, therefore, is leave a log line confirming that
 * the device booted and that MiraclePay SMS forwarding is armed. It shows no
 * UI, posts no notification, and starts no service.</p>
 *
 * <p>Declared in {@code AndroidManifest.xml} with intent filters for
 * {@link Intent#ACTION_BOOT_COMPLETED} and the HTC/older-OEM
 * {@code android.intent.action.QUICKBOOT_POWERON} action.</p>
 */
public class BootReceiver extends BroadcastReceiver {

    private static final String TAG = "MiraclePay.Boot";

    /** Fast-boot action broadcast by some OEM devices instead of BOOT_COMPLETED. */
    private static final String ACTION_QUICKBOOT_POWERON =
            "android.intent.action.QUICKBOOT_POWERON";

    @Override
    public void onReceive(Context context, Intent intent) {
        if (intent == null) {
            return;
        }

        String action = intent.getAction();
        if (Intent.ACTION_BOOT_COMPLETED.equals(action)
                || ACTION_QUICKBOOT_POWERON.equals(action)) {
            // Nothing to re-register and nothing to reschedule:
            //  - SmsReceiver is manifest-registered, so it is already active.
            //  - WorkManager restores persisted pending forwards on its own.
            Log.i(TAG, "Device booted (" + action + "); "
                    + "MiraclePay SMS forwarding is armed.");
        }
    }
}
