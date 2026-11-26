# Backup & restore

MiraclePay v0.0.1 — operator guide (Wave 27).

## What state exists, and where

| State | Where it lives | Backed up by |
|---|---|---|
| Users, merchants, transactions, invoices, refunds, settlements, subscriptions, API keys, audit log | PostgreSQL when `DATABASE_URL` is set (schema: `migrations/`) | `scripts/backup.sh` |
| The same, in **in-memory mode** (no `DATABASE_URL`) | Process RAM only — **gone on restart, nothing to back up** | — (do not run production this way) |
| Webhook delivery queue (worker file store) | JSON file at the worker's configured path | Copy the file alongside the DB dump |
| Secrets & configuration | Environment (`.env`, never committed) | Your secret manager; `backup.sh` records only variable **names** |

## Taking a backup

```bash
DATABASE_URL=postgres://user:pass@host:5432/miraclepay ./scripts/backup.sh
# → backups/<UTC timestamp>/database.sql.gz, env.list, git.txt
```

Requires `pg_dump` (Debian/Ubuntu: `apt install postgresql-client`).

**Schedule it** (example: nightly at 03:15, keep 14 days):

```cron
15 3 * * *  cd /opt/miraclepay && DATABASE_URL=... ./scripts/backup.sh && find backups -maxdepth 1 -type d -mtime +14 -exec rm -rf {} +
```

Copy backups off the host (object storage, another machine) — a
backup that only exists on the same disk is not a backup.

## Restoring

```bash
DATABASE_URL=postgres://user:pass@host:5432/miraclepay \
  ./scripts/restore.sh backups/20261126T031500Z
```

- The target database must be **empty** (the script checks and
  refuses otherwise) or pass `--force-empty` to drop the public
  schema first — destructive, double-check the URL.
- `--yes` skips the interactive confirmation (for automation).
- After restoring, restart the API: on boot it runs pending SQLx
  migrations, which is a no-op for a same-version restore and the
  upgrade path for an older one (see `docs/operations/upgrade.md`
  when it lands at Wave 47; until then, restore only into the same
  version).
- Also restore the webhook delivery file if the worker used one,
  and re-provide the environment variables listed in `env.list`.

## Restore drill (do this quarterly)

1. Create a scratch database.
2. Restore the latest backup into it.
3. Start the API against it (`DATABASE_URL=...scratch...`) and run
   the smoke flow: login → list merchants → list transactions.
4. Record the drill date and outcome. An untested backup is a hope,
   not a backup.

## Targets (single-host v0.0.1)

- **RPO**: 24 h with the nightly schedule (tighten the cron if the
  payment volume justifies it).
- **RTO**: ~15 minutes (provision DB → restore → restart → smoke).
