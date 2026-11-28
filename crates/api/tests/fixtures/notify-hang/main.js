// Test fixture: notify() loops forever. The engine's manifest
// timeout must interrupt it in the background; the settle path must
// never await it.
function notify(event) {
  while (true) {}
}
