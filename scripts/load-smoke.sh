#!/bin/bash
# Load smoke (Wave 129): 200 health requests + 50 openapi fetches
# against a running server; every response must be 200. Not a
# benchmark — a "does it fall over immediately" gate.
set -euo pipefail
BASE="${LOAD_BASE:-http://127.0.0.1:18099}"
seq 1 200 | xargs -P 10 -I{} sh -c "curl -sf -o /dev/null '$BASE/health/live' || echo FAIL{}" | grep -c FAIL && { echo "load-smoke: failures under light load"; exit 1; } || true
seq 1 50 | xargs -P 5 -I{} sh -c "curl -sf -o /dev/null '$BASE/openapi.json' || echo FAIL{}" | grep -c FAIL && { echo "load-smoke: openapi failures"; exit 1; } || true
echo "load-smoke: OK (250 requests, 0 failures)"
