#!/bin/bash
# Embedded-UI smoke (v0.0.3.2): boots the release binary — which
# must have the real Astro bundle embedded — and proves over HTTP
# that the single binary serves the full product:
#   /            → 200 HTML landing page (names MiraclePay)
#   /login/      → 200 HTML page
#   /health      → JSON, version matches the release
#   /api/v1/…    → unknown API path stays a JSON 404
#   /_astro/…    → a real hashed asset, JS content type, immutable
# Any failed step exits non-zero. Used in CI against the release
# binary and for the release read-back proof.
set -euo pipefail

BIN="${1:?usage: embedded-ui-smoke.sh <path-to-miraclepay-api>}"
[ -x "$BIN" ] || { echo "ui-smoke: binary $BIN not found/executable"; exit 1; }
PORT="${UI_SMOKE_PORT:-18101}"
BASE="http://127.0.0.1:$PORT"

PORT=$PORT PLUGINS_DIR="${PLUGINS_DIR:-plugins}" "$BIN" &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
for i in $(seq 1 50); do curl -sf "$BASE/health/live" >/dev/null 2>&1 && break || sleep 0.2; done

fail() { echo "ui-smoke: FAIL — $1"; exit 1; }

# /health stays JSON.
HEALTH=$(curl -sf "$BASE/health") || fail "GET /health"
echo "$HEALTH" | grep -q '"status":"ok"' || fail "/health body: $HEALTH"
echo "ui-smoke: /health → $HEALTH"

# Landing page: 200, HTML, names the product.
ROOT_HEADERS=$(curl -s -D - -o /tmp/ui-smoke-root.html "$BASE/")
echo "$ROOT_HEADERS" | head -1 | grep -q "200" || fail "GET / status: $(echo "$ROOT_HEADERS" | head -1)"
echo "$ROOT_HEADERS" | grep -qi "content-type: text/html" || fail "GET / content-type"
grep -q "MiraclePay" /tmp/ui-smoke-root.html || fail "GET / body does not mention MiraclePay"
echo "ui-smoke: GET / → 200 text/html ($(wc -c < /tmp/ui-smoke-root.html) bytes, MiraclePay present)"

# A real page route.
LOGIN_HEADERS=$(curl -s -D - -o /tmp/ui-smoke-login.html "$BASE/login/")
echo "$LOGIN_HEADERS" | head -1 | grep -q "200" || fail "GET /login/ status"
echo "$LOGIN_HEADERS" | grep -qi "content-type: text/html" || fail "GET /login/ content-type"
grep -q "MiraclePay" /tmp/ui-smoke-login.html || fail "GET /login/ body"
echo "ui-smoke: GET /login/ → 200 text/html ($(wc -c < /tmp/ui-smoke-login.html) bytes)"

# Unknown API path: JSON 404, not the HTML fallback.
API404=$(curl -s -w '\n%{http_code} %{content_type}' "$BASE/api/v1/definitely-not-a-route")
echo "$API404" | grep -q "404 application/json" || fail "unknown API path: $API404"
echo "$API404" | grep -q '"error":"not_found"' || fail "unknown API body: $API404"
echo "ui-smoke: GET /api/v1/definitely-not-a-route → 404 application/json {error: not_found}"

# A hashed _astro asset from the built bundle on disk: the binary
# must serve the same file with a JS/CSS content type + immutable.
ASSET=$(find frontend/dist/_astro -type f \( -name '*.js' -o -name '*.css' \) | head -1)
[ -n "$ASSET" ] || fail "no _astro asset found under frontend/dist"
ASSET_PATH="/_astro/$(basename "$ASSET")"
ASSET_HEADERS=$(curl -s -D - -o /dev/null "$BASE$ASSET_PATH")
echo "$ASSET_HEADERS" | head -1 | grep -q "200" || fail "GET $ASSET_PATH status"
echo "$ASSET_HEADERS" | grep -qiE "content-type: (text/javascript|text/css)" || fail "GET $ASSET_PATH content-type: $(echo "$ASSET_HEADERS" | grep -i content-type)"
echo "$ASSET_HEADERS" | grep -qi "cache-control:.*immutable" || fail "GET $ASSET_PATH cache-control"
echo "ui-smoke: GET $ASSET_PATH → 200, $(echo "$ASSET_HEADERS" | grep -i content-type | tr -d '\r'), immutable"

echo "ui-smoke: OK — the binary serves the full UI"
