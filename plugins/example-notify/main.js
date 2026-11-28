// Example Notify — example notification plugin.
//
// The core calls notify(event) for payment lifecycle events
// (payment.completed, payment.failed) for every enabled
// notification plugin. Dispatch is fire-and-forget: the return
// value is ignored by the pipeline (it is still visible when the
// operation is called through the admin invoke endpoint), and a
// plugin that throws or times out never blocks or fails the
// payment.
//
// This example does the two things a real channel plugin (email,
// SMS, chat webhook, …) would build on: it records the event in
// the plugin log, and it keeps a running per-event-type counter
// in its own KV store. It sends nothing anywhere — it has no
// allowed_hosts, so http.fetch cannot reach the network at all.

function notify(event) {
  // The event name lives in the `event` key of the argument
  // object (see docs/plugins.md for the exact argument shape).
  var type = (event && (event.event || event.event_type || event.type)) || "unknown";
  var countKey = "count:" + type;
  var count = (kv.get(countKey) || 0) + 1;
  kv.set(countKey, count);
  kv.set("last_event", event || null);
  log.info("notify " + type + " (#" + count + ")" +
    (event && event.merchant_id ? " merchant " + event.merchant_id : "") +
    (event && event.transaction_id ? " txn " + event.transaction_id : ""));
  return { ok: true, event_type: type, count: count };
}
