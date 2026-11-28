// Test fixture: stamps a marker onto the webhook payload.
//
// Dispatch contract (plugins.rs `transform_webhook_payload`): the
// plugin is invoked as transform_webhook({event_type, payload,
// merchant_id}) and a returned JSON object REPLACES the payload.
// So this stamps the marker onto the payload object and returns
// exactly that object. The other entry-point names return the same
// marked payload, in case dispatch ever calls them with either the
// envelope or the bare payload.
function marked_payload(input) {
  var payload =
    input && typeof input === "object" && input.payload && typeof input.payload === "object"
      ? input.payload
      : input;
  if (payload && typeof payload === "object") {
    payload.plugin_transformed = "transform-marker-v1";
    payload.transformed_by = "transform-marker";
  }
  return payload;
}

function transform_webhook(input) { return marked_payload(input); }
function transform(input) { return marked_payload(input); }
function transform_payload(input) { return marked_payload(input); }
function transform_event(input) { return marked_payload(input); }
