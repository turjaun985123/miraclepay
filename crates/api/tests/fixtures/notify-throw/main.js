// Test fixture: notify() always throws.
function notify(event) {
  throw new Error("notify-throw fixture: deliberate failure");
}
