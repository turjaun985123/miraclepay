#!/bin/bash
# Backup/restore drill (Wave 91): proves a pg_dump of a live
# database restores cleanly into a fresh one and the data survives.
# Uses TEST_DATABASE_URL (CI) or DATABASE_URL. Creates and drops
# two scratch databases; never touches the source database beyond
# creating one marker table inside a scratch source DB.
set -euo pipefail
URL="${TEST_DATABASE_URL:-${DATABASE_URL:-}}"
if [ -z "$URL" ]; then echo "restore-drill: no TEST_DATABASE_URL/DATABASE_URL — skipped"; exit 0; fi
if ! command -v pg_dump >/dev/null || ! command -v psql >/dev/null; then
  if [ "${CI:-}" = "true" ]; then echo "restore-drill: psql/pg_dump missing in CI" >&2; exit 1; fi
  echo "restore-drill: psql/pg_dump not installed — skipped"; exit 0
fi
BASE="${URL%/*}"
SRC="$BASE/mp_drill_src"
DST="$BASE/mp_drill_dst"
cleanup() {
  psql "$BASE/postgres" -qc "DROP DATABASE IF EXISTS mp_drill_src" >/dev/null 2>&1 || true
  psql "$BASE/postgres" -qc "DROP DATABASE IF EXISTS mp_drill_dst" >/dev/null 2>&1 || true
}
cleanup
psql "$BASE/postgres" -qc "CREATE DATABASE mp_drill_src"
psql "$BASE/postgres" -qc "CREATE DATABASE mp_drill_dst"
psql "$SRC" -qc "CREATE TABLE drill_marker (id int primary key, note text); INSERT INTO drill_marker VALUES (1, 'wave-91');"
pg_dump "$SRC" > /tmp/mp-drill.sql
psql "$DST" -q < /tmp/mp-drill.sql
GOT=$(psql "$DST" -tAc "SELECT note FROM drill_marker WHERE id = 1")
cleanup
if [ "$GOT" = "wave-91" ]; then echo "restore-drill: OK (dump → restore → data verified)"; else echo "restore-drill: FAILED (got '$GOT')" >&2; exit 1; fi
