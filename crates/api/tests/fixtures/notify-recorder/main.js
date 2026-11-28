// Test fixture: records each notify(event) call in KV.
// Counts are kept per event type under "count:<type>", plus a
// "count:total", and the last raw event under "last:<type>", so the
// test can poll the plugin KV instead of sleeping blindly.
function notify(event) {
  var type = "unknown";
  if (event) {
    type = event.type || event.event_type || event.event || "unknown";
  }
  var key = "count:" + type;
  var n = kv.get(key) || 0;
  kv.set(key, n + 1);
  var total = kv.get("count:total") || 0;
  kv.set("count:total", total + 1);
  kv.set("last:" + type, event);
  log.info("notify recorded " + type);
  return { ok: true };
}
