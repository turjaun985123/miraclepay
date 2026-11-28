// Test fixture: every transform entry point throws immediately.
function fail(input) {
  throw new Error("transform-throw fixture: deliberate failure");
}

function transform(input) { return fail(input); }
function transform_webhook(input) { return fail(input); }
function transform_payload(input) { return fail(input); }
function transform_event(input) { return fail(input); }
