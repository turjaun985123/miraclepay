package com.miraclepay.app;

// MiraclePay v0.0.1 — SettingsActivity
// Clean-room implementation.
//
// Settings screen of the MiraclePay SMS forwarder. It lets the user edit
// the three values that point this device at their MiraclePay server —
// the server base URL, the device API key issued from the dashboard, and
// an optional human-readable merchant label — shows the device's stable
// generated ID (read-only, so it can be copied into the dashboard), saves
// the edits, and can probe the configured server before saving.
//
// ConfigStore API used here (same package):
//   ConfigStore(Context), String getServerUrl(), String getDeviceKey(),
//   String getMerchantLabel(), String getDeviceId(),
//   void save(String serverUrl, String deviceKey, String merchantLabel)
// ApiClient API used here (same package):
//   static ApiClient.Result testConnection(ConfigStore)
//   — blocking network I/O, never throws; Result carries success,
//     statusCode and errorMessage. Always call it off the UI thread.
//
// Layout R.layout.activity_settings must provide:
//   EditText  @id/serverUrlEdit      — server base URL
//   EditText  @id/deviceKeyEdit      — device API key
//   EditText  @id/merchantLabelEdit  — optional merchant label
//   TextView  @id/deviceIdText       — read-only device ID
//   Button    @id/saveButton         — validate + persist the fields
//   Button    @id/testButton         — probe the saved server URL

import android.os.Bundle;
import android.widget.Button;
import android.widget.EditText;
import android.widget.TextView;
import android.widget.Toast;

import androidx.appcompat.app.ActionBar;
import androidx.appcompat.app.AppCompatActivity;

public class SettingsActivity extends AppCompatActivity {

    private ConfigStore configStore;

    private EditText serverUrlEdit;
    private EditText deviceKeyEdit;
    private EditText merchantLabelEdit;
    private TextView deviceIdText;
    private Button saveButton;
    private Button testButton;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_settings);

        // This screen is reached from MainActivity, so offer Up navigation
        // back to it.
        ActionBar actionBar = getSupportActionBar();
        if (actionBar != null) {
            actionBar.setDisplayHomeAsUpEnabled(true);
        }

        configStore = new ConfigStore(this);

        serverUrlEdit = findViewById(R.id.serverUrlEdit);
        deviceKeyEdit = findViewById(R.id.deviceKeyEdit);
        merchantLabelEdit = findViewById(R.id.merchantLabelEdit);
        deviceIdText = findViewById(R.id.deviceIdText);
        saveButton = findViewById(R.id.saveButton);
        testButton = findViewById(R.id.testButton);

        // Prefill every editable field from the stored configuration so the
        // user edits what is actually in effect, not a blank form.
        serverUrlEdit.setText(configStore.getServerUrl());
        deviceKeyEdit.setText(configStore.getDeviceKey());
        merchantLabelEdit.setText(configStore.getMerchantLabel());

        // The device ID is generated once by ConfigStore and never edited
        // here; it is shown so it can be registered in the dashboard.
        deviceIdText.setText(configStore.getDeviceId());

        saveButton.setOnClickListener(v -> saveSettings());
        testButton.setOnClickListener(v -> testConnection());
    }

    @Override
    public boolean onSupportNavigateUp() {
        finish();
        return true;
    }

    /**
     * Validates the fields and persists them through
     * {@link ConfigStore#save(String, String, String)}.
     *
     * <p>The server URL is the only field with a format rule: it must be an
     * absolute {@code http://} or {@code https://} URL, otherwise nothing is
     * saved and the user is told why. The device key and merchant label are
     * stored as entered (ConfigStore trims surrounding whitespace).</p>
     */
    private void saveSettings() {
        String serverUrl = textOf(serverUrlEdit);
        String deviceKey = textOf(deviceKeyEdit);
        String merchantLabel = textOf(merchantLabelEdit);

        if (!serverUrl.startsWith("http://") && !serverUrl.startsWith("https://")) {
            Toast.makeText(this,
                    "Server URL must start with http:// or https://",
                    Toast.LENGTH_LONG).show();
            serverUrlEdit.requestFocus();
            return;
        }

        configStore.save(serverUrl, deviceKey, merchantLabel);
        Toast.makeText(this, "Saved", Toast.LENGTH_SHORT).show();
    }

    /**
     * Probes the currently stored server with
     * {@link ApiClient#testConnection(ConfigStore)} on a background thread
     * and toasts the outcome, including the HTTP status when one arrived.
     *
     * <p>The probe uses the saved configuration, so unsaved edits are not
     * tested — the button is disabled while the probe runs so it cannot be
     * started twice.</p>
     */
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
            });
        }, "miraclepay-test-connection").start();
    }

    /** Returns the trimmed text of an EditText, never {@code null}. */
    private static String textOf(EditText edit) {
        CharSequence text = edit.getText();
        return text == null ? "" : text.toString().trim();
    }

    private static boolean isEmpty(String value) {
        return value == null || value.trim().isEmpty();
    }
}
