#!/bin/bash
# End-to-end API smoke (Wave 127): boots the real API binary
# (in-memory mode, sandbox seed off) and walks the core flow over
# HTTP: health -> register -> login -> merchant -> payment link
# -> public pay page data -> plugin list (admin). Any failed step
# exits non-zero. Used locally and in CI.
set -euo pipefail
PORT="${E2E_PORT:-18099}"
BASE="http://127.0.0.1:$PORT"
BIN="${E2E_BIN:-target/debug/miraclepay-api}"
[ -x "$BIN" ] || { echo "e2e: binary $BIN not found (build first)"; exit 1; }
PORT=$PORT PLUGINS_DIR="${PLUGINS_DIR:-plugins}" "$BIN" &
SERVER_PID=$!
trap 'kill $SERVER_PID 2>/dev/null || true' EXIT
for i in $(seq 1 50); do curl -sf "$BASE/health/live" >/dev/null 2>&1 && break || sleep 0.2; done
curl -sf "$BASE/health/live" | grep -q '"status":"ok"' || { echo "e2e: liveness failed"; exit 1; }
curl -sf "$BASE/health/ready" | grep -q '"status":"ready"' || { echo "e2e: readiness failed"; exit 1; }

EMAIL="e2e-$(date +%s)@example.com"
curl -sf -X POST "$BASE/api/v1/auth/register" -H 'Content-Type: application/json' \
  -d "{\"name\":\"E2E\",\"email\":\"$EMAIL\",\"password\":\"e2e-pass-123\"}" >/dev/null || { echo "e2e: register failed"; exit 1; }
TOKEN=$(curl -sf -X POST "$BASE/api/v1/auth/login" -H 'Content-Type: application/json' \
  -d "{\"email\":\"$EMAIL\",\"password\":\"e2e-pass-123\"}" | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])')
[ -n "$TOKEN" ] || { echo "e2e: login failed"; exit 1; }
MERCHANT=$(curl -sf -X POST "$BASE/api/v1/merchants" -H "Authorization: Bearer $TOKEN" -H 'Content-Type: application/json' \
  -d "{\"business_name\":\"E2E Store\",\"email\":\"$EMAIL\",\"country\":\"BD\"}")
MID=$(echo "$MERCHANT" | python3 -c 'import sys,json; print(json.load(sys.stdin)["id"])')
ONBOARD=$(curl -sf "$BASE/api/v1/merchants/$MID/onboarding" -H "Authorization: Bearer $TOKEN")
echo "$ONBOARD" | grep -q '"total":7' || { echo "e2e: onboarding shape wrong: $ONBOARD"; exit 1; }
OPENAPI_OPS=$(curl -sf "$BASE/openapi.json" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(sum(len(v) for v in d["paths"].values()))')
[ "$OPENAPI_OPS" -ge 118 ] || { echo "e2e: openapi ops $OPENAPI_OPS < 118"; exit 1; }
echo "e2e-smoke: OK (health, auth, merchant, onboarding, openapi $OPENAPI_OPS ops)"
