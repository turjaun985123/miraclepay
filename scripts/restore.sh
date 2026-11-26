#!/usr/bin/env bash
# MiraclePay restore (Wave 27).
#
# Restores a backup directory produced by scripts/backup.sh into
# the database named by $DATABASE_URL.
#
#   DATABASE_URL=postgres://... ./scripts/restore.sh backups/<stamp> [--yes]
#
# Safety: without --yes the script asks for confirmation, because
# the restore DROPS and recreates the public schema contents via
# the dump (pg_dump output includes DROP statements only when taken
# with --clean; our dumps rely on restoring into an EMPTY database —
# the script therefore refuses a non-empty target unless --force-empty
# is also given, which drops the public schema first).
set -euo pipefail

BACKUP_DIR="${1:-}"
CONFIRM=""
FORCE_EMPTY=""
for arg in "$@"; do
  case "$arg" in
    --yes) CONFIRM="yes" ;;
    --force-empty) FORCE_EMPTY="yes" ;;
  esac
done

if [ -z "$BACKUP_DIR" ] || [ ! -f "$BACKUP_DIR/database.sql.gz" ]; then
  echo "Usage: DATABASE_URL=... $0 <backup-dir> [--yes] [--force-empty]" >&2
  exit 1
fi
if [ -z "${DATABASE_URL:-}" ]; then
  echo "ERROR: DATABASE_URL is not set." >&2
  exit 1
fi
if ! command -v psql >/dev/null 2>&1; then
  echo "ERROR: psql not found (install postgresql-client)." >&2
  exit 1
fi

TABLE_COUNT="$(psql "$DATABASE_URL" -tAc \
  "select count(*) from information_schema.tables where table_schema='public'")"
if [ "$TABLE_COUNT" != "0" ] && [ -z "$FORCE_EMPTY" ]; then
  echo "ERROR: target database already has $TABLE_COUNT tables." >&2
  echo "Restore into an empty database, or pass --force-empty to drop" >&2
  echo "the public schema first (DESTRUCTIVE)." >&2
  exit 1
fi

if [ -z "$CONFIRM" ]; then
  read -r -p "Restore $BACKUP_DIR into the target database? [y/N] " reply
  case "$reply" in
    y|Y) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

if [ -n "$FORCE_EMPTY" ]; then
  echo "Dropping and recreating the public schema…"
  psql "$DATABASE_URL" -c "drop schema public cascade; create schema public;"
fi

echo "Restoring…"
gunzip -c "$BACKUP_DIR/database.sql.gz" | psql "$DATABASE_URL" -v ON_ERROR_STOP=1

echo "Restore complete. Restart the API so it re-runs migrations/checks"
echo "against the restored data (see docs/operations/backup-restore.md)."
