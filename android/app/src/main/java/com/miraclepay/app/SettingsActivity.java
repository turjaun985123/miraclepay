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
//   EditText  @id/allowedSendersEdit — allowed senders, one per line (Wave 109)
//   TextView  @id/deviceIdText       — read-only device ID
//   Button    @id/saveButton         — validate + persist the fields
//   Button    @id/testButton         — probe the saved server URL

import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.Context;
import android.os.Bundle;
import android.text.Editable;
import android.text.TextWatcher;
import android.widget.Button;
import android.widget.EditText;
import android.widget.TextView;

import com.google.android.material.snackbar.Snackbar;
import com.google.android.material.textfield.TextInputLayout;

import androidx.appcompat.app.ActionBar;
import androidx.appcompat.app.AppCompatActivity;
import androidx.appcompat.widget.Toolbar;

public class SettingsActivity extends AppCompatActivity {

    private ConfigStore configStore;

    private TextInputLayout serverUrlLayout;
    private EditText serverUrlEdit;
    private EditText deviceKeyEdit;
    private EditText merchantLabelEdit;
    private EditText allowedSendersEdit;
    private TextView deviceIdText;
    private Button saveButton;
    private Button testButton;
    private Button copyDeviceIdButton;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_settings);

        // MaterialToolbar (Waves 189-194) replaces the old reliance on
        // the app theme's ActionBar (the theme is NoActionBar, so Up
        // navigation silently never appeared before).
        Toolbar toolbar = findViewById(R.id.settingsToolbar);
        setSupportActionBar(toolbar);
        ActionBar actionBar = getSupportActionBar();
        if (actionBar != null) {
            actionBar.setDisplayHomeAsUpEnabled(true);
        }

        configStore = new ConfigStore(this);

        serverUrlLayout = findViewById(R.id.serverUrlLayout);
        serverUrlEdit = findViewById(R.id.serverUrlEdit);
        deviceKeyEdit = findViewById(R.id.deviceKeyEdit);
        merchantLabelEdit = findViewById(R.id.merchantLabelEdit);
        allowedSendersEdit = findViewById(R.id.allowedSendersEdit);
        deviceIdText = findViewById(R.id.deviceIdText);
        saveButton = findViewById(R.id.saveButton);
        testButton = findViewById(R.id.testButton);
        copyDeviceIdButton = findViewById(R.id.copyDeviceIdButton);

        // Prefill every editable field from the stored configuration so the
        // user edits what is actually in effect, not a blank form.
        serverUrlEdit.setText(configStore.getServerUrl());
        deviceKeyEdit.setText(configStore.getDeviceKey());
        merchantLabelEdit.setText(configStore.getMerchantLabel());
        // Wave 109: show the allowed senders one per line, whatever
        // separator the stored value used.
        StringBuilder allowedText = new StringBuilder();
        for (String sender : configStore.getAllowedSenderList()) {
            if (allowedText.length() > 0) {
                allowedText.append('\n');
            }
            allowedText.append(sender);
        }
        allowedSendersEdit.setText(allowedText.toString());

        // The device ID is generated once by ConfigStore and never edited
        // here; it is shown so it can be registered in the dashboard.
        deviceIdText.setText(configStore.getDeviceId());

        saveButton.setOnClickListener(v -> saveSettings());
        testButton.setOnClickListener(v -> testConnection());

        // Copy device ID (Waves 189-194): its whole purpose is
        // registration in the dashboard, so make it one tap.
        copyDeviceIdButton.setOnClickListener(v -> {
            ClipboardManager clipboard =
                    (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
            if (clipboard != null) {
                clipboard.setPrimaryClip(ClipData.newPlainText(
                        getString(R.string.device_id_label), configStore.getDeviceId()));
                showSnackbar(getString(R.string.copied));
            }
        });

        // Clear the URL error as soon as the user edits the field.
        serverUrlEdit.addTextChangedListener(new TextWatcher() {
            @Override public void beforeTextChanged(CharSequence t, int a, int b, int c) { }
            @Override public void onTextChanged(CharSequence t, int a, int b, int c) {
                serverUrlLayout.setError(null);
            }
            @Override public void afterTextChanged(Editable e) { }
        });
    }

    /** Transient feedback (Waves 189-194): Snackbar replaces Toast. */
    private void showSnackbar(String message) {
        Snackbar.make(findViewById(android.R.id.content), message,
                Snackbar.LENGTH_LONG).show();
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
            // Field-level error (Waves 189-194), not a toast: the
            // problem is in this field, so the field says so.
            serverUrlLayout.setError(getString(R.string.server_url_error));
            serverUrlEdit.requestFocus();
            return;
        }
        serverUrlLayout.setError(null);

        configStore.save(serverUrl, deviceKey, merchantLabel);
        // Wave 109: the allowed-senders field is saved separately —
        // ConfigStore.save(...) does not cover it. Empty = allow all.
        configStore.setAllowedSenders(textOf(allowedSendersEdit));
        showSnackbar(getString(R.string.saved));
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
        showSnackbar(getString(R.string.testing));

        new Thread(() -> {
            // testConnection performs network I/O and never throws; its
            // Result is turned into a short, human-readable toast here.
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
                showSnackbar(resultText);
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
