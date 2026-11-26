# Operator handbook

The consolidation layer for running MiraclePay day to day. It
does not repeat the specialist guides — it tells you which one
to open, when, and what your routine is.

| Topic | Go to |
|---|---|
| Incidents, env reference, per-symptom fixes | [runbook.md](runbook.md) |
| Backups, restore, restore drills, RPO/RTO | [backup-restore.md](backup-restore.md) |
| Upgrades & rollback | [upgrade-guide.md](upgrade-guide.md) |
| Sandbox/demo mode | [sandbox.md](sandbox.md) |
| Load testing & capacity measurement | [performance.md](performance.md) |
| Deployment (Docker) | [../deployment/docker.md](../deployment/docker.md) |
| Deployment (bare metal) | [../deployment/bare-metal.md](../deployment/bare-metal.md) |
| Deployment (VPS/cPanel) | [../deployment/vps-cpanel.md](../deployment/vps-cpanel.md) |

## Roles

Keep these separate even if one person holds two of them:

- **Operator** — owns the deployment: deploys, backups,
  restores, env/secrets, the routines below. Only role that
  touches `DATABASE_URL`, `JWT_SECRET`, or the host.
- **Admin (in-app)** — uses the admin API/dashboard: audit log,
  KYC queue, fraud flags, platform settings, webhook requeues.
  Seeded by `ADMIN_EMAIL`/`ADMIN_PASSWORD` on first boot.
- **Merchant support** — first contact for merchants. Works from
  the dashboard and the runbook's merchant-facing checks (SMS
  forwarder state, webhook endpoint health). Never asks a
  merchant for their password or API secret.

## Daily routine

From runbook.md §Routine, in order:

1. Confirm the backup ran and produced a new directory under
   `backups/` (backup-restore.md). A silent backup failure is
   the highest-severity routine finding.
2. `curl -s https://your-host/health` — expect `status ok`,
   `db postgres`, `sandbox false`. Either of the last two wrong
   is an incident, not a note.
3. Glance at `/metrics`: webhook deliveries with
   `status="deadletter"` and failed-transaction counts.
4. Skim the request logs for repeated 5xx (logs are one JSON
   line per request on stdout; find a request by its
   `x-request-id`).

## Weekly routine

1. Review the admin audit log (`GET /api/v1/admin/audit`).
2. Work the KYC queue (`GET /api/v1/admin/kyc/pending`).
3. Review fraud flags (`GET /api/v1/admin/fraud/flagged`) —
   advisory only; nothing auto-blocks (runbook.md).
4. Check disk headroom for the Postgres volume and the
   `backups/` directory; confirm old backups are being pruned
   and copied off-host.
5. Confirm the coming week's deploy window, if any, against
   upgrade-guide.md's before-you-upgrade list.

## Monthly / quarterly

- **Quarterly:** run the restore drill in backup-restore.md
  (restore into a scratch database, boot the API against it,
  smoke-test) and record the date and outcome.
- **After any significant traffic change:** re-measure capacity
  per performance.md — release build, load generator on a
  different host. The numbers in that file are floors from a
  shared VM, not capacity claims for your deployment.
- Review who still holds operator/admin access; rotate
  `JWT_SECRET` and admin credentials on personnel change
  (rotating `JWT_SECRET` invalidates all live sessions).

## Incident index

All fixes live in runbook.md §Incidents; this is the index.

| Symptom | Runbook section |
|---|---|
| Webhooks piling up / dead letters rising | "Webhooks piling up / dead letters" |
| Notification dispatch returns "SMTP not configured" (409) | "'SMTP not configured' on notification dispatch" |
| Merchant payments stuck Pending | "A merchant says payments are stuck Pending" (usually the SMS forwarder) |
| Clients hitting HTTP 429 | "Rate-limit 429s" |
| Suspicious payments | "Fraud flags" |
| `/health` shows `db: in_memory` in production | runbook.md §"Know your deployment" — `DATABASE_URL` not connecting; state is volatile |
| `/health` shows `sandbox: true` publicly | Same section — unset `SANDBOX_MODE`, restart against a fresh store |
| Bad release | upgrade-guide.md §Rollback |

## Escalation

Escalate to whoever owns the codebase/vendor relationship when:

- An incident is not in the index above, or the runbook fix
  does not clear it within one attempt.
- A restore or rollback is needed outside a planned upgrade.
- Fraud flags suggest a pattern across merchants, not a single
  payment.

Bring with the escalation, so it can be acted on without a
round trip:

- The `/health` output (version, `db`, `sandbox`).
- The failing request's `x-request-id` and the matching JSON
  log lines.
- What changed last: deploy, env change, gateway credential
  change — with timestamps.
- For merchant issues: merchant id, transaction ids, and
  whether the SMS forwarder app shows a queued/failed outbox.

Never include secrets (`JWT_SECRET`, gateway credentials,
database passwords) in an escalation ticket or chat log.

## First week checklist

For a new deployment, in order:

- [ ] Deployed via one route only (Docker **or** bare metal —
      docs/deployment/) and reachable over HTTPS through a
      reverse proxy.
- [ ] Every default secret replaced: `POSTGRES_PASSWORD` =
      password inside `DATABASE_URL`, long random `JWT_SECRET`,
      non-default `ADMIN_EMAIL`/`ADMIN_PASSWORD`.
- [ ] `/health` verified: `status ok`, `db postgres`,
      `sandbox false`.
- [ ] Frontend built with the real public `PUBLIC_API_URL`
      (build-time value — see the limitation note in
      docker.md); login works from a browser on another machine.
- [ ] SMTP set (`SMTP_HOST` + `SMTP_FROM`) and one notification
      dispatched successfully — or the gap recorded as a known
      limitation.
- [ ] Each gateway you intend to use configured via env or the
      dashboard; unconfigured gateways answer NotConfigured by
      design, so test one real checkout per live gateway.
- [ ] One sandbox-mode rehearsal done on a **separate** local
      instance (sandbox.md), never on the public one.
- [ ] Nightly backup scheduled, first backup verified, backup
      copied off-host, restore drill date put on the calendar.
- [ ] One webhook delivered end-to-end to a real merchant
      endpoint and signature verification confirmed
      (docs/webhooks.md).
- [ ] Daily/weekly routines above assigned to a named operator,
      and this handbook linked from wherever the team keeps its
      run links.
