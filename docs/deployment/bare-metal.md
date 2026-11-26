# Bare-metal deployment

Run the Rust binaries directly on a Linux host, with PostgreSQL
16 and systemd. Use the Docker route (docker.md) unless you have
a reason not to — this guide is the manual equivalent of what
`docker-compose.yml` + the Dockerfiles do.

## Requirements

- A current stable Rust toolchain (the Docker builder uses
  `rust:1-slim-bookworm`; the workspace is edition 2021).
- PostgreSQL 16 (the Compose deployment uses
  `postgres:16-alpine`), with `pg_dump` available for backups
  (Debian/Ubuntu: `apt install postgresql-client`).
- Node.js for the frontend build — the frontend image builds
  with `node:24-alpine`; `frontend/package.json` requires Astro
  `^7.4.13`.

## Build

```bash
cargo build --release -p miraclepay-api -p miraclepay-worker
strip target/release/miraclepay-api target/release/miraclepay-worker
```

(These are the exact package names and strip step from the root
`Dockerfile`.) Install the binaries somewhere stable, e.g.:

```bash
install -m 0755 target/release/miraclepay-api target/release/miraclepay-worker /opt/miraclepay/bin/
```

Frontend:

```bash
cd frontend
PUBLIC_API_URL=https://your-public-api-host npm ci
PUBLIC_API_URL=https://your-public-api-host npm run build   # → frontend/dist/
```

`PUBLIC_API_URL` is read at build time
(`frontend/src/lib/api.ts`) and defaults to
`http://localhost:8080`. Set it to the URL browsers will use to
reach the API before building. Serve `frontend/dist/` with any
static file server (nginx below, or the reverse proxy of your
choice).

## PostgreSQL

Create the role and database the Compose file would create
(database `miraclepay`, user `miraclepay`):

```sql
CREATE USER miraclepay WITH PASSWORD '<choose-a-password>';
CREATE DATABASE miraclepay OWNER miraclepay;
```

There is no migration command to run by hand: when
`DATABASE_URL` is set, the API creates the pool and runs the
pending migrations (`migrations/0001_core.sql` …
`0004_ops.sql`) on startup. If `DATABASE_URL` is unset — or the
connection fails — the API starts on the in-memory store
instead and reports `"db":"in_memory"` on `/health`. That mode
loses all state on restart; production means Postgres.

## Environment file

Create `/etc/miraclepay/miraclepay.env` (root-owned, mode
`0600`). Variable semantics are defined by `.env.example` and
the runbook; the minimum for production:

```ini
DATABASE_URL=postgres://miraclepay:<password>@127.0.0.1:5432/miraclepay
JWT_SECRET=<long random value, e.g. openssl rand -hex 32>
PORT=8080
ADMIN_EMAIL=<admin email>
ADMIN_PASSWORD=<strong password>
PUBLIC_API_URL=https://your-public-api-host
```

Notes:

- `JWT_SECRET` unset → the API falls back to a fixed development
  secret. Always set it.
- `ADMIN_EMAIL`/`ADMIN_PASSWORD` seed the first admin only when
  both are set; change the password after first login per your
  own policy and keep the env values in your secret store.
- Do not set `SANDBOX_MODE` on a public deployment
  (docs/operations/sandbox.md).

## systemd units

Create a system user first:

```bash
useradd --system --create-home --uid 10001 miraclepay
```

`/etc/systemd/system/miraclepay-api.service`:

```ini
[Unit]
Description=MiraclePay API
After=network.target postgresql.service
Wants=postgresql.service

[Service]
User=miraclepay
WorkingDirectory=/opt/miraclepay
EnvironmentFile=/etc/miraclepay/miraclepay.env
ExecStart=/opt/miraclepay/bin/miraclepay-api
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

`/etc/systemd/system/miraclepay-worker.service`:

```ini
[Unit]
Description=MiraclePay worker
After=network.target postgresql.service
Wants=postgresql.service

[Service]
User=miraclepay
WorkingDirectory=/opt/miraclepay
EnvironmentFile=/etc/miraclepay/miraclepay.env
ExecStart=/opt/miraclepay/bin/miraclepay-worker
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

Current state of the worker binary, so expectations are set
correctly: `miraclepay-worker` runs a 60-second heartbeat loop;
the job engines (webhook delivery, settlements, notifications)
live in the worker library and are driven from the API process /
a scheduler in production. Run the service anyway so the
deployment shape matches Compose, but do not treat its log line
as proof that background jobs ran.

Enable and verify:

```bash
systemctl daemon-reload
systemctl enable --now miraclepay-api miraclepay-worker
curl -s localhost:8080/health   # expect status ok, db postgres, sandbox false
journalctl -u miraclepay-api -f # JSON request logs on stdout land here
```

## Reverse proxy & TLS

The API listens on `0.0.0.0:$PORT` (default `8080`) and ships no
TLS. Put a reverse proxy (nginx, Caddy, Apache) in front:

- Proxy the API hostname/path to `http://127.0.0.1:8080`.
- Serve `frontend/dist/` as static files on the public hostname.
- Terminate HTTPS at the proxy (e.g. a Let's Encrypt
  certificate).
- Firewall the host so only the proxy ports (80/443) are public;
  keep `8080` and PostgreSQL's `5432` reachable only locally /
  from the proxy.

## Upgrade

Follow docs/operations/upgrade-guide.md. In bare-metal terms:

1. Back up first (docs/operations/backup-restore.md —
   `scripts/backup.sh` with `DATABASE_URL` set).
2. `git pull`, rebuild the release binaries, reinstall them
   into `/opt/miraclepay/bin/`.
3. `systemctl restart miraclepay-api miraclepay-worker` — the
   API applies pending migrations on startup.
4. Verify `/health` (`status ok`, `db postgres`,
   `sandbox false`), one checkout end-to-end, one webhook
   delivery.

Rollback: reinstall the previous binaries; if the release
migrated the database, also restore the pre-upgrade backup per
the upgrade guide.
