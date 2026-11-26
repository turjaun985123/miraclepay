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
import android.widget.Button;
import android.widget.TextView;
import android.widget.Toast;

import androidx.annotation.NonNull;
import androidx.appcompat.app.AppCompatActivity;
import androidx.appcompat.widget.SwitchCompat;
import androidx.core.app.ActivityCompat;
import androidx.core.content.ContextCompat;

public class MainActivity extends AppCompatActivity {

    /** Request code for the RECEIVE_SMS / READ_SMS runtime permission ask. */
    static final int REQUEST_SMS_PERMISSIONS = 1001;

    private static final String[] SMS_PERMISSIONS = {
            Manifest.permission.RECEIVE_SMS,
            Manifest.permission.READ_SMS
    };

    private ConfigStore configStore;

    private TextView statusText;
    private TextView configSummaryText;
    private SwitchCompat enableSwitch;
    private Button settingsButton;
    private Button testButton;
    private Button permissionButton;
    private Button batteryButton;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_main);

        configStore = new ConfigStore(this);

        statusText = findViewById(R.id.statusText);
        configSummaryText = findViewById(R.id.configSummaryText);
        enableSwitch = findViewById(R.id.enableSwitch);
        settingsButton = findViewById(R.id.settingsButton);
        testButton = findViewById(R.id.testButton);
        permissionButton = findViewById(R.id.permissionButton);
        batteryButton = findViewById(R.id.batteryButton);

        settingsButton.setOnClickListener(v ->
                startActivity(new Intent(this, SettingsActivity.class)));

        testButton.setOnClickListener(v -> testConnection());

        permissionButton.setOnClickListener(v -> requestSmsPermissions());
        batteryButton.setOnClickListener(v -> requestBatteryExemption());

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

        statusText.setText(
                "Forwarding: " + (enabled ? "On" : "Off") + "\n"
                        + "SMS permission: " + (smsGranted ? "Granted" : "Not granted") + "\n"
                        + "Status: " + (configured ? "Configured" : "Not configured"));

        configSummaryText.setText(
                "Server: " + (isEmpty(serverUrl) ? "(not set)" : serverUrl) + "\n"
                        + "Device ID: " + (isEmpty(deviceId) ? "(not set)" : deviceId));

        if (enableSwitch.isChecked() != enabled) {
            // The listener re-saves the same value it was loaded from,
            // so syncing the switch here is harmless.
            enableSwitch.setChecked(enabled);
        }

        // Show the permission button prominently only while a permission
        // is missing; once granted it has nothing left to do.
        permissionButton.setVisibility(smsGranted ? View.GONE : View.VISIBLE);
        if (!smsGranted) {
            permissionButton.bringToFront();
        }

        // Battery exemption (Wave 42): without it, aggressive OEM
        // battery savers can defer forwards for hours. Show the
        // button only while not exempt.
        boolean batteryExempt = isBatteryExempt();
        batteryButton.setVisibility(batteryExempt ? View.GONE : View.VISIBLE);
        statusText.append("\nBattery: " + (batteryExempt ? "Unrestricted" : "Restricted"));
    }

    /** Runs ApiClient.testConnection off the UI thread and toasts the result. */
    private void testConnection() {
        testButton.setEnabled(false);
        Toast.makeText(this, "Testing connection…", Toast.LENGTH_SHORT).show();

        new Thread(() -> {
            // testConnection performs network I/O and never throws; its
            // Result is turned into a short, human-readable toast here.
            ApiClient.Result result = ApiClient.testConnection(configStore);
            final String toastText;
            if (result.isSuccess()) {
                toastText = result.getStatusCode() > 0
                        ? "Connection OK (HTTP " + result.getStatusCode() + ")."
                        : "Connection OK.";
            } else if (!isEmpty(result.getErrorMessage())) {
                toastText = "Connection failed: " + result.getErrorMessage();
            } else if (result.getStatusCode() > 0) {
                toastText = "Connection failed (HTTP " + result.getStatusCode() + ").";
            } else {
                toastText = "Connection failed.";
            }
            runOnUiThread(() -> {
                testButton.setEnabled(true);
                Toast.makeText(this, toastText, Toast.LENGTH_LONG).show();
                refreshStatus();
            });
        }, "miraclepay-test-connection").start();
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
            Toast.makeText(this,
                    "Open Settings → Battery and set MiraclePay to Unrestricted.",
                    Toast.LENGTH_LONG).show();
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

        Toast.makeText(this,
                allGranted
                        ? "SMS permission granted."
                        : "SMS permission denied — MiraclePay cannot forward payment SMS.",
                Toast.LENGTH_LONG).show();
        refreshStatus();
    }

    private static boolean isEmpty(String value) {
        return value == null || value.trim().isEmpty();
    }
}
