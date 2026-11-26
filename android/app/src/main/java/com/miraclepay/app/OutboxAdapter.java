package com.miraclepay.app;

// MiraclePay — OutboxAdapter (Waves 189-194).
// Read-only RecyclerView adapter over OutboxStore.list(): the queue
// becomes a list of real messages (sender, snippet, relative time,
// Waiting/Failed status) instead of a count in a paragraph. Retry
// stays with the existing Retry-all button; no data-layer changes.

import android.content.Context;
import android.text.format.DateUtils;
import android.util.TypedValue;
import android.view.LayoutInflater;
import android.view.View;
import android.view.ViewGroup;
import android.widget.TextView;

import java.util.ArrayList;
import java.util.List;

import androidx.annotation.NonNull;
import androidx.recyclerview.widget.RecyclerView;

public class OutboxAdapter extends RecyclerView.Adapter<OutboxAdapter.Holder> {

    private final List<OutboxStore.Entry> entries = new ArrayList<>();

    public void submitList(List<OutboxStore.Entry> list) {
        entries.clear();
        if (list != null) {
            entries.addAll(list);
        }
        notifyDataSetChanged();
    }

    @NonNull
    @Override
    public Holder onCreateViewHolder(@NonNull ViewGroup parent, int viewType) {
        View view = LayoutInflater.from(parent.getContext())
                .inflate(R.layout.item_outbox, parent, false);
        return new Holder(view);
    }

    @Override
    public void onBindViewHolder(@NonNull Holder holder, int position) {
        OutboxStore.Entry entry = entries.get(position);
        Context context = holder.itemView.getContext();

        String sender = entry.sender == null || entry.sender.trim().isEmpty()
                ? "?" : entry.sender.trim();
        holder.avatarText.setText(sender.substring(0, 1).toUpperCase());
        holder.senderText.setText(sender);
        holder.bodyText.setText(entry.body == null ? "" : entry.body);

        CharSequence when = entry.receivedAtMillis > 0L
                ? DateUtils.getRelativeTimeSpanString(entry.receivedAtMillis,
                        System.currentTimeMillis(), DateUtils.MINUTE_IN_MILLIS)
                : "";
        String meta = when.toString();
        if (entry.simSlot > 0) {
            meta = meta.isEmpty() ? "SIM " + entry.simSlot : meta + " · SIM " + entry.simSlot;
        }
        holder.metaText.setText(meta);

        boolean failed = OutboxStore.STATUS_FAILED.equals(entry.status);
        holder.statusText.setText(failed
                ? context.getString(R.string.status_failed)
                : context.getString(R.string.status_waiting));
        holder.statusText.setTextColor(resolveColor(context,
                failed ? com.google.android.material.R.attr.colorError
                        : com.google.android.material.R.attr.colorPrimary));
    }

    @Override
    public int getItemCount() {
        return entries.size();
    }

    private static int resolveColor(Context context, int attr) {
        TypedValue value = new TypedValue();
        context.getTheme().resolveAttribute(attr, value, true);
        return value.data;
    }

    static final class Holder extends RecyclerView.ViewHolder {
        final TextView avatarText;
        final TextView senderText;
        final TextView bodyText;
        final TextView metaText;
        final TextView statusText;

        Holder(@NonNull View itemView) {
            super(itemView);
            avatarText = itemView.findViewById(R.id.avatarText);
            senderText = itemView.findViewById(R.id.senderText);
            bodyText = itemView.findViewById(R.id.bodyText);
            metaText = itemView.findViewById(R.id.metaText);
            statusText = itemView.findViewById(R.id.statusText);
        }
    }
}
