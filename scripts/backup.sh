#!/usr/bin/env bash
# MiraclePay backup (Wave 27).
#
# Creates a timestamped backup directory under ./backups/ containing:
#   - database.sql.gz — pg_dump of $DATABASE_URL (the system of
#     record when the API runs with a database configured)
#   - env.list        — the NAMES (never values) of the environment
#     variables present, so a restore knows what must be provided
#   - git.txt         — the exact commit the backup was taken from
#
# In-memory mode (no DATABASE_URL): there is NO durable state to
# back up — the script says so and exits non-zero rather than
# producing an empty, misleading backup.
#
# Usage: DATABASE_URL=postgres://... ./scripts/backup.sh [out-dir]
set -euo pipefail

OUT_ROOT="${1:-./backups}"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
DEST="$OUT_ROOT/$STAMP"
mkdir -p "$DEST"

if [ -z "${DATABASE_URL:-}" ]; then
  echo "ERROR: DATABASE_URL is not set." >&2
  echo "Without it the API runs in in-memory mode and has no durable" >&2
  echo "state to back up. Set DATABASE_URL and re-run." >&2
  exit 1
fi

if ! command -v pg_dump >/dev/null 2>&1; then
  echo "ERROR: pg_dump not found. Install postgresql-client, or run" >&2
  echo "this inside the compose network: docker compose exec api ..." >&2
  exit 1
fi

echo "Dumping database…"
pg_dump "$DATABASE_URL" | gzip -9 > "$DEST/database.sql.gz"

echo "Recording environment variable names (values are never copied)…"
env | cut -d= -f1 | sort > "$DEST/env.list"

if git rev-parse --short HEAD >/dev/null 2>&1; then
  git rev-parse HEAD > "$DEST/git.txt"
fi

echo "Backup written to $DEST"
ls -la "$DEST"
