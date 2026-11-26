# Sandbox mode

Sandbox mode seeds a small demo dataset so you can evaluate
MiraclePay locally in minutes — no registration, no real money.

## Run it

```bash
SANDBOX_MODE=1 JWT_SECRET=dev-secret cargo run -p miraclepay-api
# or with Docker:
SANDBOX_MODE=1 docker compose up
```

Then open the frontend, and log in with the demo account:

- **Email:** `demo@miraclepay.local`
- **Password:** `demo1234`

You get a merchant ("Demo Store (Sandbox)"), 14 days of demo
transactions (the dashboard chart has shape immediately), a
payment link (`demo-store-pay`, ৳500 fixed) and a sent invoice.

## The rules

- Seeding happens **only** when the user store is empty and
  `SANDBOX_MODE` is set (`1` / `true` / `yes`). Restarts never
  duplicate the demo data. There is no HTTP endpoint that seeds.
- The demo password is **published on purpose** — that is exactly
  why sandbox mode must never run on a public deployment. Anyone
  who can reach a sandbox deployment can log in as the demo owner.
- `GET /health` reports `"sandbox": true` while the mode is on,
  so a misconfigured deployment is discoverable at a glance.
- All demo transactions carry `"demo": true` in their metadata
  and `DEMO…` provider ids. Demo money is not real money; nothing
  in sandbox mode talks to a real gateway (gateway adapters stay
  env-gated as usual).

## Turning it off

Unset `SANDBOX_MODE` and start against a fresh database/store.
Sandbox mode off = the seed code never runs; the demo account
simply does not exist.
