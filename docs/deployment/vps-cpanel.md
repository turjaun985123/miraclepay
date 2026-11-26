# VPS / cPanel deployment notes

Short version: MiraclePay needs a host that can run long-lived
processes — the Rust API binary, the worker binary, and
PostgreSQL 16. That decides which hosting products work.

## Shared cPanel hosting: not supported

A typical shared cPanel account cannot run MiraclePay:

- No root access and no systemd/Docker — you cannot install or
  supervise the `miraclepay-api` / `miraclepay-worker` binaries
  as always-on services.
- Shared hosts prohibit or kill long-lived background processes,
  which both binaries are.
- You cannot bind the application to its own port (`8080` for
  the API) or run your own PostgreSQL 16 instance with the
  connection string the API expects (`DATABASE_URL`).
- cPanel's application helpers (e.g. "Setup Node.js App") target
  specific runtimes; there is no equivalent for a compiled Rust
  service plus a worker plus Postgres.

Do not try to work around this with cron-started binaries or
PHP proxies; the result would be an API that dies between
requests and loses its worker entirely. Use one of the routes
below instead.

## What works

### 1. A plain VPS (recommended)

Any VPS where you have root/SSH:

- **Docker route** — docker.md. Simplest; Compose brings up
  Postgres, API, worker and frontend together.
- **Bare-metal route** — bare-metal.md. Binaries under systemd,
  host PostgreSQL 16.

### 2. A VPS with cPanel/WHM installed (root access)

If the server is yours and you administer it through WHM,
cPanel is just the panel on top of a normal Linux VPS — the
application does not run "in" cPanel, it runs beside it:

- Install and run MiraclePay by the Docker or bare-metal route
  above, as root, outside any cPanel account's directory tree.
- Use cPanel/WHM only for what it is good at here: creating the
  domain/subdomain, DNS, and (if you terminate TLS there) the
  certificate. The Rust services themselves are managed with
  `docker compose` or `systemctl`, not from cPanel.
- Watch for port conflicts with services cPanel already runs
  before assigning the API/frontend ports.

## Domain + HTTPS

1. Point the domain (or a subdomain such as `pay.example.com`)
   at the VPS with an A record.
2. Run a reverse proxy (nginx or Apache) on the VPS:
   - Public hostname → frontend (Compose publishes it on
     `${FRONTEND_PORT:-4321}`; bare-metal serves
     `frontend/dist/` as static files).
   - API traffic → `http://127.0.0.1:8080` (the API's `PORT`,
     default `8080`).
3. Terminate HTTPS at the proxy: a Let's Encrypt certificate
   (certbot, or cPanel AutoSSL on a WHM server for the proxied
   vhost). The application itself ships no TLS.
4. Set `PUBLIC_API_URL` to the public HTTPS API URL and rebuild
   the frontend with it — the value is baked into the static
   build at build time (`frontend/src/lib/api.ts`, default
   `http://localhost:8080`). The Compose frontend image does not
   receive this variable; see the known-limitation note in
   docker.md.

## Ports & firewall

| Port | What | Expose publicly? |
|---|---|---|
| `80` / `443` | Reverse proxy (HTTP → HTTPS, HTTPS) | Yes |
| `8080` | API (`PORT`) | No — proxy to it locally |
| `4321` | Frontend under Compose (`FRONTEND_PORT`) | No — proxy to it locally |
| `5432` | PostgreSQL (published by Compose) | **Never** |
| `6379` | Redis (only with `--profile redis`) | **Never** |

On a WHM server, apply the same rules in its firewall (e.g.
CSF) in addition to any cloud-provider firewall.

## Before going live on any VPS route

- Replace every default in `.env` / the env file:
  `POSTGRES_PASSWORD` and the password inside `DATABASE_URL`
  (they must match), `JWT_SECRET` (long random value),
  `ADMIN_EMAIL` / `ADMIN_PASSWORD`.
- Confirm `curl -s localhost:8080/health` reports
  `"db":"postgres"` and `"sandbox":false`. `in_memory` means
  the database is not connected and all state will vanish on
  restart; `sandbox: true` means the public demo login is live.
- Schedule backups (docs/operations/backup-restore.md) and copy
  them off the VPS.
