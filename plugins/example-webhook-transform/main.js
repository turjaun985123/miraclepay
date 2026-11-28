// Example Webhook Transform — example webhook_transformer plugin.
//
// When a merchant has opted in to webhook transformation, the
// core calls transform_webhook({event_type, payload,
// merchant_id}) while building each outbound webhook payload,
// before the payload is signed and queued for delivery.
//
// Return value contract:
//   - return an object  → it REPLACES the payload that is signed
//                         and delivered;
//   - return null       → the original payload is delivered
//                         unchanged.
// A transformer that throws or times out also leaves the
// original payload in place — transformation can never break
// webhook delivery (bounded by a 2-second cap for opted-in
// merchants).
//
// This example is deliberately harmless: it returns a copy of
// the payload with two additions —
//   transformed_by: "example-webhook-transform"
//   meta: { event_type: <the event type> }
// A real transformer might rename fields, drop internal fields,
// or nest the payload under a key a merchant's legacy receiver
// expects. Note the signature covers the TRANSFORMED payload, so
// receivers verify against what was actually sent.

function transform_webhook(req) {
  var payload = req.payload;
  if (!payload || typeof payload !== "object") {
    return null;
  }
  var out = {};
  var keys = Object.keys(payload);
  for (var i = 0; i < keys.length; i++) {
    out[keys[i]] = payload[keys[i]];
  }
  out.transformed_by = "example-webhook-transform";
  out.meta = { event_type: req.event_type };
  log.info("transformed webhook " + req.event_type +
    " for merchant " + req.merchant_id);
  return out;
}
