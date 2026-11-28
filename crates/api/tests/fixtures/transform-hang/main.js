// Test fixture: every transform entry point loops forever; only
// the manifest timeout can cut it off.
function spin(input) {
  while (true) {}
}

function transform(input) { return spin(input); }
function transform_webhook(input) { return spin(input); }
function transform_payload(input) { return spin(input); }
function transform_event(input) { return spin(input); }
