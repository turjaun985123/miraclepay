// MiraclePay v0.0.1 — OutboxStore.java
// Clean-room implementation. Original code written for MiraclePay.
package com.miraclepay.app;

import android.content.Context;
import android.content.SharedPreferences;

import org.json.JSONArray;
import org.json.JSONException;
import org.json.JSONObject;

import java.util.ArrayList;
import java.util.List;
import java.util.UUID;

/**
 * The app-side forwarding outbox (Wave 43).
 *
 * <p>WorkManager's queue is durable but invisible: when a forward
 * kept failing, the user had no way to see it or retry it. This
 * store mirrors every queued message (sender, body, timestamps,
 * attempt count, last error, status) in SharedPreferences so the
 * main screen can show exactly what is waiting and offer a manual
 * retry.
 *
 * <p>Lifecycle: {@code SmsReceiver} adds an entry as PENDING when
 * it enqueues the WorkManager request; {@code SmsForwardWorker}
 * removes the entry on success, marks it FAILED on a permanent
 * (4xx / unconfigured) failure, and bumps the attempt count when
 * WorkManager will retry. The "Retry now" button re-enqueues every
 * PENDING/FAILED entry and flips FAILED back to PENDING.
 *
 * <p>Bodies are the user's own SMS on the user's own phone; the UI
 * shows sender + time only, never the body. The store is capped at
 * {@link #MAX_ENTRIES} entries (oldest FAILED entries drop first)
 * so a dead backend cannot grow it without bound.
 */
public final class OutboxStore {

    /** SharedPreferences file for the outbox. */
    public static final String PREFS_NAME = "miraclepay_outbox";

    private static final String KEY_ENTRIES = "entries";
    private static final int MAX_ENTRIES = 200;

    /** Entry status: waiting to be forwarded (or retrying). */
    public static final String STATUS_PENDING = "pending";
    /** Entry status: permanently failed until manually retried. */
    public static final String STATUS_FAILED = "failed";

    /** One queued message. */
    public static final class Entry {
        public final String id;
        public final String sender;
        public final String body;
        public final long receivedAtMillis;
        public final int simSlot;
        public int attempts;
        public String status;
        public String lastError;

        Entry(String id, String sender, String body, long receivedAtMillis, int simSlot,
              int attempts, String status, String lastError) {
            this.id = id;
            this.sender = sender;
            this.body = body;
            this.receivedAtMillis = receivedAtMillis;
            this.simSlot = simSlot;
            this.attempts = attempts;
            this.status = status;
            this.lastError = lastError;
        }
    }

    private final SharedPreferences prefs;

    public OutboxStore(Context context) {
        this.prefs = context.getApplicationContext()
                .getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE);
    }

    /** Add a message as PENDING; returns its entry id. */
    public synchronized String add(String sender, String body, long receivedAtMillis, int simSlot) {
        List<Entry> entries = list();
        String id = UUID.randomUUID().toString();
        entries.add(new Entry(id, sender, body, receivedAtMillis, simSlot,
                0, STATUS_PENDING, null));
        trim(entries);
        save(entries);
        return id;
    }

    /** All entries, oldest first. */
    public synchronized List<Entry> list() {
        List<Entry> out = new ArrayList<>();
        String raw = prefs.getString(KEY_ENTRIES, "[]");
        try {
            JSONArray array = new JSONArray(raw);
            for (int i = 0; i < array.length(); i++) {
                JSONObject obj = array.getJSONObject(i);
                out.add(new Entry(
                        obj.getString("id"),
                        obj.getString("sender"),
                        obj.getString("body"),
                        obj.getLong("received_at"),
                        obj.optInt("sim_slot", -1),
                        obj.optInt("attempts", 0),
                        obj.optString("status", STATUS_PENDING),
                        obj.isNull("last_error") ? null : obj.optString("last_error", null)));
            }
        } catch (JSONException e) {
            // A corrupt store must not wedge forwarding: start clean.
            return new ArrayList<>();
        }
        return out;
    }

    /** Remove an entry (delivered). */
    public synchronized void markDone(String id) {
        List<Entry> entries = list();
        entries.removeIf(entry -> entry.id.equals(id));
        save(entries);
    }

    /** Record a permanent failure with its reason. */
    public synchronized void markFailed(String id, String error) {
        mutate(id, entry -> {
            entry.status = STATUS_FAILED;
            entry.lastError = error;
            entry.attempts += 1;
        });
    }

    /** Record a transient failure (WorkManager will retry). */
    public synchronized void markAttempt(String id, String error) {
        mutate(id, entry -> {
            entry.attempts += 1;
            entry.lastError = error;
        });
    }

    /** Flip a FAILED entry back to PENDING for a manual retry. */
    public synchronized void markRetrying(String id) {
        mutate(id, entry -> {
            entry.status = STATUS_PENDING;
            entry.lastError = null;
        });
    }

    /** Count entries in a status. */
    public synchronized int countByStatus(String status) {
        int count = 0;
        for (Entry entry : list()) {
            if (status.equals(entry.status)) {
                count++;
            }
        }
        return count;
    }

    private interface Mutation {
        void apply(Entry entry);
    }

    private synchronized void mutate(String id, Mutation mutation) {
        List<Entry> entries = list();
        for (Entry entry : entries) {
            if (entry.id.equals(id)) {
                mutation.apply(entry);
                break;
            }
        }
        save(entries);
    }

    /** Keep the store bounded: drop oldest FAILED entries first. */
    private void trim(List<Entry> entries) {
        while (entries.size() > MAX_ENTRIES) {
            int drop = -1;
            for (int i = 0; i < entries.size(); i++) {
                if (STATUS_FAILED.equals(entries.get(i).status)) {
                    drop = i;
                    break;
                }
            }
            entries.remove(drop >= 0 ? drop : 0);
        }
    }

    private void save(List<Entry> entries) {
        JSONArray array = new JSONArray();
        for (Entry entry : entries) {
            JSONObject obj = new JSONObject();
            try {
                obj.put("id", entry.id);
                obj.put("sender", entry.sender);
                obj.put("body", entry.body);
                obj.put("received_at", entry.receivedAtMillis);
                obj.put("sim_slot", entry.simSlot);
                obj.put("attempts", entry.attempts);
                obj.put("status", entry.status);
                obj.put("last_error", entry.lastError == null
                        ? JSONObject.NULL : entry.lastError);
            } catch (JSONException ignored) {
                // Field names are constants; this cannot happen.
            }
            array.put(obj);
        }
        prefs.edit().putString(KEY_ENTRIES, array.toString()).apply();
    }
}
