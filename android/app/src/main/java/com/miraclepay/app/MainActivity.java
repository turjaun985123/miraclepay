package com.miraclepay.app;

// MiraclePay v0.0.1 — MainActivity
// Clean-room implementation.
//
// Launcher screen of the MiraclePay SMS forwarder. It shows whether the
// app is configured, where it forwards to, and whether forwarding and
// the SMS permission are on, and it wires the three actions a user needs
// day to day: open settings, test the server connection, and (re)grant
// the SMS permissions.
//
// Expected ConfigStore API (same package):
//   ConfigStore(Context), boolean isConfigured(), String getServerUrl(),
//   String getDeviceId(), boolean isEnabled(), void setEnabled(boolean)
// ApiClient API used here (same package):
//   static ApiClient.Result testConnection(ConfigStore)
//   — blocking network I/O, never throws; Result carries success,
//     statusCode and errorMessage. Always call it off the UI thread.

import android.Manifest;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.view.View;
import android.text.format.DateUtils;
import android.widget.Button;
import android.widget.TextView;

import com.google.android.material.snackbar.Snackbar;

import androidx.annotation.NonNull;
import androidx.appcompat.app.AppCompatActivity;
import androidx.appcompat.widget.SwitchCompat;
import androidx.core.app.ActivityCompat;
import androidx.core.content.ContextCompat;
import androidx.recyclerview.widget.LinearLayoutManager;
import androidx.recyclerview.widget.RecyclerView;

public class MainActivity extends AppCompatActivity {

    /** Request code for the RECEIVE_SMS / READ_SMS runtime permission ask. */
    static final int REQUEST_SMS_PERMISSIONS = 1001;

    private static final String[] SMS_PERMISSIONS = {
            Manifest.permission.RECEIVE_SMS,
            Manifest.permission.READ_SMS
    };

    private ConfigStore configStore;

    private TextView statusText;
    private View statusDot;
    private TextView configSummaryText;
    private TextView statsForwardedText;
    private TextView statsFailedText;
    private TextView statsFilteredText;
    private TextView statsLastText;
    private View permissionBanner;
    private View batteryBanner;
    private TextView queueEmptyText;
    private RecyclerView outboxList;
    private OutboxAdapter outboxAdapter;
    private View testProgress;
    private TextView testResultText;
    private SwitchCompat enableSwitch;
    private Button settingsButton;
    private Button testButton;
    private Button permissionButton;
    private Button batteryButton;
    private Button retryButton;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);

        configStore = new ConfigStore(this);

        statusText = findViewById(R.id.statusText);
        statusDot = findViewById(R.id.statusDot);
        configSummaryText = findViewById(R.id.configSummaryText);
        statsForwardedText = findViewById(R.id.statsForwardedText);
        statsFailedText = findViewById(R.id.statsFailedText);
        statsFilteredText = findViewById(R.id.statsFilteredText);
        statsLastText = findViewById(R.id.statsLastText);
        permissionBanner = findViewById(R.id.permissionBanner);
        batteryBanner = findViewById(R.id.batteryBanner);
        queueEmptyText = findViewById(R.id.queueEmptyText);
        outboxList = findViewById(R.id.outboxList);
        outboxAdapter = new OutboxAdapter();
        outboxList.setLayoutManager(new LinearLayoutManager(this));
        outboxList.setAdapter(outboxAdapter);
        testProgress = findViewById(R.id.testProgress);
        testResultText = findViewById(R.id.testResultText);
        enableSwitch = findViewById(R.id.enableSwitch);
        settingsButton = findViewById(R.id.settingsButton);
        testButton = findViewById(R.id.testButton);
        permissionButton = findViewById(R.id.permissionButton);
        batteryButton = findViewById(R.id.batteryButton);
        retryButton = findViewById(R.id.retryButton);

        settingsButton.setOnClickListener(v ->
                startActivity(new Intent(this, SettingsActivity.class)));

        testButton.setOnClickListener(v -> testConnection());

        permissionButton.setOnClickListener(v -> requestSmsPermissions());
        batteryButton.setOnClickListener(v -> requestBatteryExemption());
        retryButton.setOnClickListener(v -> retryOutbox());

        // Reflect the saved forwarding state before attaching the listener,
        // so restoring the switch does not re-save the same value.
        enableSwitch.setChecked(configStore.isEnabled());
        enableSwitch.setOnCheckedChangeListener((buttonView, isChecked) -> {
            configStore.setEnabled(isChecked);
            refreshStatus();
        });

        refreshStatus();

        // Auto-check on start: refreshStatus() already makes the
        // permission button prominent when a permission is missing.
        // The ask itself is left to the button so the system dialog
        // never appears uninvited on launch.
    }

    @Override
    protected void onResume() {
        super.onResume();
        // Settings may have changed the configuration, and the user may
        // have changed permissions in system settings while we were away.
        refreshStatus();
    }

    /**
     * Re-reads the stored configuration and permission state and updates
     * every view on this screen: configured or not, server URL, device
     * id, forwarding on/off, and SMS permission granted or not.
     */
    private void refreshStatus() {
        if (configStore == null) {
            return;
        }

        boolean configured = configStore.isConfigured();
        boolean enabled = configStore.isEnabled();
        boolean smsGranted = hasSmsPermissions();
        String serverUrl = configStore.getServerUrl();
        String deviceId = configStore.getDeviceId();

        // Hero headline + status dot (design state matrix, Waves
        // 189-194): the headline answers "is forwarding working?",
        // the banners below answer "what must I do?".
        int headline;
        int dot;
        if (!configured) {
            headline = R.string.status_needs_setup;
            dot = R.drawable.dot_grey;
        } else if (!smsGranted) {
            headline = R.string.status_blocked_permission;
            dot = R.drawable.dot_red;
        } else if (enabled) {
            headline = R.string.status_forwarding_on;
            dot = R.drawable.dot_green;
        } else {
            headline = R.string.status_paused;
            dot = R.drawable.dot_grey;
        }
        statusText.setText(headline);
        statusDot.setBackgroundResource(dot);

        configSummaryText.setText(
                getString(R.string.server_label) + ": "
                        + (isEmpty(serverUrl) ? getString(R.string.not_set) : serverUrl) + "\n"
                        + getString(R.string.device_id_label) + ": "
                        + (isEmpty(deviceId) ? getString(R.string.not_set) : deviceId));

        if (enableSwitch.isChecked() != enabled) {
            // The listener re-saves the same value it was loaded from,
            // so syncing the switch here is harmless.
            enableSwitch.setChecked(enabled);
        }

        // Banners explain the consequence, not just the action; the
        // buttons inside them keep the original IDs and listeners.
        permissionBanner.setVisibility(smsGranted ? View.GONE : View.VISIBLE);

        // Battery exemption (Wave 42): without it, aggressive OEM
        // battery savers can defer forwards for hours.
        batteryBanner.setVisibility(isBatteryExempt() ? View.GONE : View.VISIBLE);

        // Outbox (Wave 43 + 189-194): the queue is a list of real
        // messages now, fed by the existing OutboxStore.list().
        OutboxStore outbox = new OutboxStore(this);
        java.util.List<OutboxStore.Entry> entries = outbox.list();
        int pending = outbox.countByStatus(OutboxStore.STATUS_PENDING);
        int failed = outbox.countByStatus(OutboxStore.STATUS_FAILED);
        retryButton.setVisibility(pending + failed > 0 ? View.VISIBLE : View.GONE);
        boolean queueEmpty = entries.isEmpty();
        queueEmptyText.setVisibility(queueEmpty ? View.VISIBLE : View.GONE);
        outboxList.setVisibility(queueEmpty ? View.GONE : View.VISIBLE);
        if (!queueEmpty) {
            outboxAdapter.submitList(entries);
        }

        // Activity tiles (Wave 110 counters, redesigned 189-194):
        // each value is set individually — no string concatenation.
        StatsStore stats = new StatsStore(this);
        statsForwardedText.setText(String.valueOf(stats.getTotalForwarded()));
        statsFailedText.setText(String.valueOf(stats.getTotalFailed()));
        statsFilteredText.setText(String.valueOf(stats.getTotalFiltered()));
        StringBuilder last = new StringBuilder();
        long lastSuccess = stats.getLastSuccessAtMillis();
        if (lastSuccess > 0L) {
            last.append(getString(R.string.last_success)).append(" · ")
                    .append(formatStatsTime(lastSuccess));
        }
        long lastFailureAt = stats.getLastFailureAtMillis();
        if (lastFailureAt > 0L) {
            if (last.length() > 0) {
                last.append("\n");
            }
            String reason = stats.getLastFailureReason();
            last.append(getString(R.string.last_failure)).append(" · ")
                    .append(formatStatsTime(lastFailureAt))
                    .append(isEmpty(reason) ? "" : " — " + reason);
        }
        if (last.length() == 0) {
            last.append(getString(R.string.no_forwards_yet));
        }
        statsLastText.setText(last.toString());
    }

    /** Relative time for a stats timestamp ("2h ago"), or "—" when unset. */
    private static String formatStatsTime(long millis) {
        if (millis <= 0L) {
            return "—";
        }
        return DateUtils.getRelativeTimeSpanString(millis,
                System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS).toString();
    }

    /** Transient feedback (Waves 189-194): Snackbar replaces Toast. */
    private void showSnackbar(String message) {
        Snackbar.make(findViewById(android.R.id.content), message,
                Snackbar.LENGTH_LONG).show();
    }

    /** Re-enqueue every waiting/failed outbox entry (Wave 43). */
    private void retryOutbox() {
        OutboxStore outbox = new OutboxStore(this);
        int requeued = 0;
        for (OutboxStore.Entry entry : outbox.list()) {
            if (OutboxStore.STATUS_FAILED.equals(entry.status)) {
                outbox.markRetrying(entry.id);
            }
            Forwarder.enqueue(this, entry.id, entry.sender, entry.body,
                    entry.receivedAtMillis, entry.simSlot);
            requeued++;
        }
        showSnackbar(requeued == 0
                ? getString(R.string.queue_empty_toast)
                : getResources().getQuantityString(
                        R.plurals.retrying_messages, requeued, requeued));
        refreshStatus();
    }

    /**
     * Runs ApiClient.testConnection off the UI thread. Feedback is a
     * progress indicator while it runs, then a durable inline result
     * line (plus a Snackbar with Retry on failure) — Waves 189-194
     * retired the Toast-only feedback.
     */
    private void testConnection() {
        testButton.setEnabled(false);
        testProgress.setVisibility(View.VISIBLE);
        testResultText.setVisibility(View.GONE);

        new Thread(() -> {
            ApiClient.Result result = ApiClient.testConnection(configStore);
            final String resultText;
            if (result.isSuccess()) {
                resultText = result.getStatusCode() > 0
                        ? getString(R.string.connection_ok) + " (HTTP " + result.getStatusCode() + ")."
                        : getString(R.string.connection_ok) + ".";
            } else if (!isEmpty(result.getErrorMessage())) {
                resultText = getString(R.string.connection_failed) + ": " + result.getErrorMessage();
            } else if (result.getStatusCode() > 0) {
                resultText = getString(R.string.connection_failed) + " (HTTP " + result.getStatusCode() + ").";
            } else {
                resultText = getString(R.string.connection_failed) + ".";
            }
            runOnUiThread(() -> {
                testButton.setEnabled(true);
                testProgress.setVisibility(View.GONE);
                testResultText.setText(resultText);
                testResultText.setTextColor(resolveThemeColor(
                        result.isSuccess()
                                ? com.google.android.material.R.attr.colorPrimary
                                : com.google.android.material.R.attr.colorError));
                testResultText.setVisibility(View.VISIBLE);
                if (!result.isSuccess()) {
                    Snackbar.make(findViewById(android.R.id.content), resultText,
                                    Snackbar.LENGTH_LONG)
                            .setAction("Retry", v -> testConnection())
                            .show();
                }
                refreshStatus();
            });
        }, "miraclepay-test-connection").start();
    }

    private int resolveThemeColor(int attr) {
        android.util.TypedValue value = new android.util.TypedValue();
        getTheme().resolveAttribute(attr, value, true);
        return value.data;
    }

    private boolean hasSmsPermissions() {
        for (String permission : SMS_PERMISSIONS) {
            if (ContextCompat.checkSelfPermission(this, permission)
                    != PackageManager.PERMISSION_GRANTED) {
                return false;
            }
        }
        return true;
    }

    private void requestSmsPermissions() {
        // Android 13+ (Wave 42): POST_NOTIFICATIONS is a runtime
        // permission too — without it the forwarding notification
        // (and foreground-service notice) cannot be shown. Ask for
        // it together with the SMS pair.
        if (android.os.Build.VERSION.SDK_INT >= 33
                && ContextCompat.checkSelfPermission(this, Manifest.permission.POST_NOTIFICATIONS)
                        != PackageManager.PERMISSION_GRANTED) {
            ActivityCompat.requestPermissions(
                    this,
                    new String[] {
                            Manifest.permission.RECEIVE_SMS,
                            Manifest.permission.READ_SMS,
                            Manifest.permission.POST_NOTIFICATIONS
                    },
                    REQUEST_SMS_PERMISSIONS);
            return;
        }
        ActivityCompat.requestPermissions(this, SMS_PERMISSIONS, REQUEST_SMS_PERMISSIONS);
    }

    /** Whether the app is exempt from battery optimization. */
    private boolean isBatteryExempt() {
        if (android.os.Build.VERSION.SDK_INT < 23) {
            return true;
        }
        android.os.PowerManager powerManager =
                (android.os.PowerManager) getSystemService(POWER_SERVICE);
        return powerManager != null
                && powerManager.isIgnoringBatteryOptimizations(getPackageName());
    }

    /** Open the system dialog asking for a battery exemption. */
    private void requestBatteryExemption() {
        try {
            Intent intent = new Intent(
                    android.provider.Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS);
            intent.setData(android.net.Uri.parse("package:" + getPackageName()));
            startActivity(intent);
        } catch (Exception e) {
            showSnackbar("Open Settings → Battery and set MiraclePay to Unrestricted.");
        }
    }

    @Override
    public void onRequestPermissionsResult(int requestCode,
                                           @NonNull String[] permissions,
                                           @NonNull int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode != REQUEST_SMS_PERMISSIONS) {
            return;
        }

        boolean allGranted = grantResults.length > 0;
        for (int result : grantResults) {
            if (result != PackageManager.PERMISSION_GRANTED) {
                allGranted = false;
                break;
            }
        }

        showSnackbar(allGranted
                ? "SMS permission granted."
                : getString(R.string.banner_permission_denied));
        refreshStatus();
    }

    private static boolean isEmpty(String value) {
        return value == null || value.trim().isEmpty();
    }
}
