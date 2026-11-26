# Docker deployment

The supported self-hosted route. `docker-compose.yml` runs the
full stack from two images built in this repo: the root
`Dockerfile` (API + worker binaries) and `Dockerfile.frontend`
(Astro static site served by nginx).

## Services

| Service | Image | Host port | Notes |
|---|---|---|---|
| `postgres` | `postgres:16-alpine` | `5432` | Primary database; data in the `pgdata` volume |
| `redis` | `redis:7-alpine` | `6379` | Optional — only starts with `--profile redis`; data in `redisdata` |
| `api` | `miraclepay-api:0.0.2` | `${PORT:-8080}` → `8080` | Runs `miraclepay-api` (the image default command) |
| `worker` | `miraclepay-api:0.0.2` | — | Same image, command overridden to `miraclepay-worker` |
| `frontend` | `miraclepay-frontend:0.0.2` | `${FRONTEND_PORT:-4321}` → `80` | nginx serving the built static site |

The `api` and `worker` services wait for Postgres to report
healthy (`pg_isready`) before starting. All services use
`restart: unless-stopped`.

## Configure

```bash
cp .env.example .env   # then edit the secrets
```

Values that must be changed from the shipped defaults before any
real deployment:

- `POSTGRES_PASSWORD` — password for the Postgres container.
- `DATABASE_URL` — the API and worker connect with this string.
  Its password **must match** `POSTGRES_PASSWORD` (`.env.example`
  says the same). The Compose default points at the `postgres`
  service host; if the two passwords drift apart, the API cannot
  connect and falls back to the in-memory store (visible as
  `"db":"in_memory"` on `/health`).
- `JWT_SECRET` — signs JWT access tokens. Generate a long random
  value, e.g. `openssl rand -hex 32` (per `.env.example`). If it
  is unset the API falls back to a fixed development secret, which
  is not acceptable in production.
- `ADMIN_EMAIL` / `ADMIN_PASSWORD` — seed the first admin account
  on first boot against an empty store. Change both before the
  first run.

Optional: `PORT` (API host port, default `8080`),
`FRONTEND_PORT` (default `4321`), `REDIS_URL`, `PUBLIC_API_URL`,
`CORS_ALLOWED_ORIGINS`, and the gateway/SMTP variables listed in
the operations runbook (`docs/operations/runbook.md`).

Do not set `SANDBOX_MODE` on a public deployment — the demo login
is public knowledge (docs/operations/sandbox.md).

## Start

```bash
docker compose up -d --build                  # everything
docker compose --profile redis up -d          # add Redis
docker compose ps
```

## Health check

The API image defines a Docker `HEALTHCHECK` that curls
`/health` every 30 s. Check directly:

```bash
curl -s localhost:8080/health
# {"status":"ok","version":"0.0.2","name":"MiraclePay","db":"postgres","sandbox":false}
```

`db` must say `postgres` and `sandbox` must be `false` in
production. Anything else is a misconfiguration — see the
runbook.

Known limitation: the frontend is a static build. Its API base
URL comes from `PUBLIC_API_URL` read at **build time**
(`frontend/src/lib/api.ts`, default `http://localhost:8080`),
and the Compose `frontend` service does not pass that variable
into the build. On a remote host, a browser loading the shipped
frontend image will therefore try to reach the API at the
visitor's own `localhost:8080`. For a public deployment, build
the frontend yourself with `PUBLIC_API_URL` set to the public
API URL (`cd frontend && npm ci && npm run build`) and serve
`frontend/dist/`, or put the whole stack behind one reverse
proxy hostname and rebuild the frontend image accordingly.

Also note: no Rust code currently reads `REDIS_URL`; Compose
passes it to the `api`/`worker` services, but Redis is not yet
wired into the backend. Run the profile only if you have another
use for it.

## Updating

Follow docs/operations/upgrade-guide.md. Short version:

```bash
git pull
docker compose build
docker compose up -d
curl -s localhost:8080/health   # expect status ok, db postgres, sandbox false
```

The API runs pending SQLx migrations on startup when
`DATABASE_URL` is set. Take a backup **before** upgrading — see
below.

## Stopping

```bash
docker compose down       # stop, keep the pgdata/redisdata volumes
docker compose down -v    # stop AND delete the volumes — destroys the database
```

## Backup

See docs/operations/backup-restore.md. `scripts/backup.sh`
produces a `pg_dump` of `DATABASE_URL` plus an env-name list and
the source commit under `backups/<UTC timestamp>/`; it refuses
to run without `DATABASE_URL`. Copy backups off the host.

## Exposure notes

- Compose publishes Postgres on host port `5432` and (with the
  profile) Redis on `6379`. On an internet-facing host, firewall
  both off or remove the port mappings — only the frontend and,
  if clients connect directly, the API port need to be reachable,
  ideally behind a TLS-terminating reverse proxy.
- The containers ship no TLS. Terminate HTTPS at a reverse proxy
  in front of the frontend/API ports.
