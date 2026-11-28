// MiraclePay v0.0.1 — Notify.java
// Clean-room implementation. Original code written for MiraclePay.
package com.miraclepay.app;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.content.Context;
import android.os.Build;

import androidx.core.app.NotificationCompat;

/**
 * Notification plumbing for the forwarder (Wave 42).
 *
 * <p>One low-importance channel, {@link #CHANNEL_FORWARDING}, used
 * for the foreground notification WorkManager shows while a
 * forward runs as a foreground (dataSync) execution, and for the
 * rare user-visible alert. Low importance: forwarding status must
 * never buzz the phone.
 */
final class Notify {

    /** Channel for forwarding/foreground notifications. */
    static final String CHANNEL_FORWARDING = "forwarding";

    /** Id of the worker's foreground notification. */
    static final int FOREGROUND_NOTIFICATION_ID = 4201;

    private Notify() {
    }

    /** Create the channel (idempotent; required on Android 8+). */
    static void ensureChannel(Context context) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return;
        }
        NotificationManager manager =
                (NotificationManager) context.getSystemService(Context.NOTIFICATION_SERVICE);
        if (manager == null || manager.getNotificationChannel(CHANNEL_FORWARDING) != null) {
            return;
        }
        NotificationChannel channel = new NotificationChannel(
                CHANNEL_FORWARDING,
                context.getString(R.string.notify_channel_name),
                NotificationManager.IMPORTANCE_LOW);
        channel.setDescription(context.getString(R.string.notify_channel_description));
        manager.createNotificationChannel(channel);
    }

    /** The quiet notification used for foreground execution. */
    static Notification foregroundNotification(Context context) {
        ensureChannel(context);
        return new NotificationCompat.Builder(context, CHANNEL_FORWARDING)
                .setContentTitle("MiraclePay")
                .setContentText(context.getString(R.string.notify_forwarding_text))
                .setSmallIcon(R.drawable.ic_stat_miraclepay)
                .setOngoing(true)
                .setPriority(NotificationCompat.PRIORITY_LOW)
                .build();
    }
}
