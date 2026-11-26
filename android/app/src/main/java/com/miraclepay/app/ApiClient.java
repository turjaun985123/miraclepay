package com.miraclepay.app;

import org.json.JSONException;
import org.json.JSONObject;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.text.SimpleDateFormat;
import java.util.Date;
import java.util.Locale;
import java.util.TimeZone;

/**
 * MiraclePay v0.0.1 — minimal HTTP client for the MiraclePay server.
 *
 * <p>Original, clean-room implementation for the Android SMS forwarder.
 * It talks to the server over plain {@link HttpURLConnection} (no
 * third-party HTTP library) and reports every outcome — success, HTTP
 * error, or transport failure — as an immutable {@link Result} instead
 * of throwing, so callers running on background threads can simply
 * inspect the result and decide whether to retry.</p>
 *
 * <p>Endpoints used:</p>
 * <ul>
 *   <li>{@code POST {serverUrl}/api/v1/sms} — forward one
 *       received SMS to the server, authenticated with the device key
 *       in the {@code x-device-key} header.</li>
 *   <li>{@code GET {serverUrl}/health} — unauthenticated liveness
 *       probe used by the settings screen's "test connection" button.</li>
 * </ul>
 */
public final class ApiClient {

    /** Path, relative to the configured server URL, for incoming SMS. */
    private static final String SMS_INCOMING_PATH = "/api/v1/sms";

    /** Path, relative to the configured server URL, for the health probe. */
    private static final String HEALTH_PATH = "/health";

    /** Header that carries the per-device credential. */
    private static final String DEVICE_KEY_HEADER = "x-device-key";

    /** TCP connect timeout, in milliseconds. */
    private static final int CONNECT_TIMEOUT_MS = 15_000;

    /** Response read timeout, in milliseconds. */
    private static final int READ_TIMEOUT_MS = 20_000;

    /** Status code used when no HTTP response was ever received. */
    public static final int NO_STATUS = -1;

    /**
     * Immutable outcome of one HTTP call.
     *
     * <p>{@link #success} is {@code true} only when the server answered
     * with a 2xx status code. On failure, {@link #errorMessage} carries
     * a short human-readable explanation suitable for logging or for
     * showing in the UI; {@link #responseBody} still holds whatever the
     * server sent back, when it sent anything at all.</p>
     */
    public static final class Result {
        /** Whether the call succeeded (HTTP 2xx). */
        public final boolean success;

        /** HTTP status code, or {@link ApiClient#NO_STATUS} if none arrived. */
        public final int statusCode;

        /** Response body as text; never {@code null} (empty when absent). */
        public final String responseBody;

        /** Failure description; {@code null} on success. */
        public final String errorMessage;

        public Result(boolean success, int statusCode, String responseBody, String errorMessage) {
            this.success = success;
            this.statusCode = statusCode;
            this.responseBody = responseBody != null ? responseBody : "";
            this.errorMessage = errorMessage;
        }

        public boolean isSuccess() {
            return success;
        }

        public int getStatusCode() {
            return statusCode;
        }

        public String getResponseBody() {
            return responseBody;
        }

        public String getErrorMessage() {
            return errorMessage;
        }

        @Override
        public String toString() {
            if (success) {
                return "Result{success, statusCode=" + statusCode + "}";
            }
            return "Result{failure, statusCode=" + statusCode
                    + ", errorMessage=" + errorMessage + "}";
        }
    }

    /** Utility class: not instantiable. */
    private ApiClient() {
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /**
     * Forwards one received SMS to the server.
     *
     * <p>Sends a JSON document of the form:</p>
     * <pre>
     * {
     *   "device_id":  "...",           // from the stored configuration
     *   "sender":     "+8801...",      // originating address of the SMS
     *   "body":       "...",           // full message text
     *   "received_at": "2026-11-26T03:14:17.000Z",  // ISO-8601, UTC
     *   "sim_slot":   0                // SIM slot the SMS arrived on
     * }
     * </pre>
     *
     * <p>This method performs network I/O and must be called from a
     * background thread, never from the main thread.</p>
     *
     * @param config          stored app configuration (server URL,
     *                        device id and device key)
     * @param sender          originating address of the SMS, may be null
     * @param body            message text, may be null
     * @param receivedAtMillis when the SMS was received, as epoch millis
     * @param simSlot         SIM slot index the SMS arrived on
     * @return the call outcome; never {@code null}, never throws
     */
    public static Result postSms(ConfigStore config, String sender, String body,
                                 long receivedAtMillis, int simSlot) {
        if (config == null) {
            return failure(NO_STATUS, "No configuration available");
        }
        String baseUrl = normalizeBaseUrl(config.getServerUrl());
        if (baseUrl == null) {
            return failure(NO_STATUS, "Server URL is not configured");
        }

        final String payload;
        try {
            JSONObject json = new JSONObject();
            json.put("device_id", nullToEmpty(config.getDeviceId()));
            json.put("sender", nullToEmpty(sender));
            json.put("body", nullToEmpty(body));
            json.put("received_at", formatIso8601Utc(receivedAtMillis));
            json.put("sim_slot", simSlot);
            payload = json.toString();
        } catch (JSONException e) {
            return failure(NO_STATUS, "Could not build request: " + describe(e));
        }

        HttpURLConnection connection = null;
        try {
            connection = openConnection(baseUrl + SMS_INCOMING_PATH, "POST");
            connection.setRequestProperty("Content-Type", "application/json");
            connection.setRequestProperty("Accept", "application/json");
            String deviceKey = config.getDeviceKey();
            if (deviceKey != null) {
                connection.setRequestProperty(DEVICE_KEY_HEADER, deviceKey);
            }
            connection.setDoOutput(true);

            byte[] bytes = payload.getBytes(StandardCharsets.UTF_8);
            connection.setFixedLengthStreamingMode(bytes.length);
            OutputStream out = connection.getOutputStream();
            try {
                out.write(bytes);
                out.flush();
            } finally {
                closeQuietly(out);
            }

            return readResult(connection);
        } catch (IOException e) {
            return failure(NO_STATUS, "Network error: " + describe(e));
        } finally {
            if (connection != null) {
                connection.disconnect();
            }
        }
    }

    /**
     * Probes the configured server with an unauthenticated health check.
     *
     * <p>Intended for the settings screen's test button: a successful
     * result means the server URL is reachable and the server is up.
     * No device key is sent with this request.</p>
     *
     * <p>This method performs network I/O and must be called from a
     * background thread, never from the main thread.</p>
     *
     * @param config stored app configuration (only the server URL is used)
     * @return the call outcome; never {@code null}, never throws
     */
    public static Result testConnection(ConfigStore config) {
        if (config == null) {
            return failure(NO_STATUS, "No configuration available");
        }
        String baseUrl = normalizeBaseUrl(config.getServerUrl());
        if (baseUrl == null) {
            return failure(NO_STATUS, "Server URL is not configured");
        }

        HttpURLConnection connection = null;
        try {
            connection = openConnection(baseUrl + HEALTH_PATH, "GET");
            connection.setRequestProperty("Accept", "application/json");
            return readResult(connection);
        } catch (IOException e) {
            return failure(NO_STATUS, "Network error: " + describe(e));
        } finally {
            if (connection != null) {
                connection.disconnect();
            }
        }
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    /**
     * Opens a connection with the shared timeout and caching settings.
     */
    private static HttpURLConnection openConnection(String url, String method) throws IOException {
        HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
        connection.setRequestMethod(method);
        connection.setConnectTimeout(CONNECT_TIMEOUT_MS);
        connection.setReadTimeout(READ_TIMEOUT_MS);
        connection.setUseCaches(false);
        connection.setInstanceFollowRedirects(false);
        return connection;
    }

    /**
     * Reads the status code and body (from the error stream on failure)
     * and folds them into a {@link Result}.
     */
    private static Result readResult(HttpURLConnection connection) throws IOException {
        int status = connection.getResponseCode();
        boolean ok = status >= 200 && status < 300;

        InputStream stream = ok ? safeInputStream(connection) : connection.getErrorStream();
        String responseBody = readFully(stream);

        if (ok) {
            return new Result(true, status, responseBody, null);
        }
        String message = responseBody.isEmpty()
                ? "Server returned HTTP " + status
                : "Server returned HTTP " + status + ": " + responseBody;
        return new Result(false, status, responseBody, message);
    }

    /** Returns the normal input stream, or null if it cannot be opened. */
    private static InputStream safeInputStream(HttpURLConnection connection) {
        try {
            return connection.getInputStream();
        } catch (IOException e) {
            return connection.getErrorStream();
        }
    }

    /** Reads a stream to the end as UTF-8 text; null/empty yields "". */
    private static String readFully(InputStream stream) throws IOException {
        if (stream == null) {
            return "";
        }
        StringBuilder builder = new StringBuilder();
        BufferedReader reader = new BufferedReader(
                new InputStreamReader(stream, StandardCharsets.UTF_8));
        try {
            String line;
            while ((line = reader.readLine()) != null) {
                builder.append(line).append('\n');
            }
        } finally {
            closeQuietly(reader);
        }
        // Trim the trailing newline added after the final line.
        int length = builder.length();
        if (length > 0 && builder.charAt(length - 1) == '\n') {
            builder.setLength(length - 1);
        }
        return builder.toString();
    }

    /**
     * Formats epoch millis as an ISO-8601 timestamp in UTC,
     * e.g. {@code 2026-11-26T03:14:17.000Z}.
     *
     * <p>Uses {@link SimpleDateFormat} rather than {@code java.time} so
     * the code also runs on older Android API levels without desugaring.
     * A fresh formatter is created per call, so this is thread-safe.</p>
     */
    static String formatIso8601Utc(long epochMillis) {
        SimpleDateFormat format = new SimpleDateFormat(
                "yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US);
        format.setTimeZone(TimeZone.getTimeZone("UTC"));
        return format.format(new Date(epochMillis));
    }

    /**
     * Trims whitespace and any trailing slashes from the configured
     * server URL so paths can be appended unconditionally.
     *
     * @return the cleaned base URL, or null if it is missing/blank
     */
    private static String normalizeBaseUrl(String serverUrl) {
        if (serverUrl == null) {
            return null;
        }
        String trimmed = serverUrl.trim();
        while (trimmed.endsWith("/")) {
            trimmed = trimmed.substring(0, trimmed.length() - 1);
        }
        return trimmed.isEmpty() ? null : trimmed;
    }

    private static Result failure(int statusCode, String errorMessage) {
        return new Result(false, statusCode, "", errorMessage);
    }

    private static String nullToEmpty(String value) {
        return value != null ? value : "";
    }

    private static String describe(Exception e) {
        String message = e.getMessage();
        return message != null && !message.isEmpty()
                ? message
                : e.getClass().getSimpleName();
    }

    private static void closeQuietly(java.io.Closeable closeable) {
        if (closeable != null) {
            try {
                closeable.close();
            } catch (IOException ignored) {
                // Closing must never mask the real outcome of the call.
            }
        }
    }
}
