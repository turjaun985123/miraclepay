# Security review — v0.0.2 (Wave 140)

A self-review of the 0.0.2 surface, with the state of each item
as shipped. "Verified" means a test or a CI gate proves it.

## Authentication & sessions
- Passwords: Argon2id with per-password CSPRNG salts; hashes
  never serialize into API responses (persistence uses a
  separate repr). Verified by tests.
- Password reset / email verification tokens: CSPRNG, stored
  SHA-256-hashed, single-use, purpose-namespaced, expiring
  (1 h / 24 h). Reset requests are anti-enumeration (always
  200) and a completed reset revokes every session.
- Refresh tokens: hashed at rest, rotating, revocable per
  session (`/auth/sessions`); suspended accounts cannot log in.
- TOTP 2FA available since 0.0.1.

## API keys & plugins
- API keys are hashed at rest; scope enforcement is real:
  PaymentsWrite to create, PaymentsRead to read, device keys
  need PaymentsWrite to settle via SMS. Revoked keys fail.
- Plugins are confined by construction: no fs/env/process;
  network only via `http.fetch` against manifest allowlisted
  hosts (private/loopback literals refused unless explicitly
  listed); per-plugin config/KV namespaces; 5 s default
  timeout + heap/output caps. Confinement is covered by tests
  (allowlist, private IP, timeout kill, namespace isolation).

## Payments integrity
- Webhook receivers verify signatures (Stripe HMAC with a
  300 s window; PayPal via the provider's verification API);
  unconfigured receivers answer 503 and settle nothing.
- The bKash callback settles only on a server-side execute
  confirming `Completed`; unconfigured callbacks are
  audit-logged and settle nothing.
- Settlement everywhere requires an amount match against the
  transaction (SMS matching, provider webhooks, the verify
  endpoint, plugin verify).
- Gateway adapters cannot fake success: without credentials
  they return NotConfigured before any network call; non-2xx
  provider answers are Provider errors.

## Transport & configuration
- SMTP credentials are refused in clear text when TLS is
  expected; STARTTLS is the default posture (`SMTP_TLS`).
- Production boot refuses a dev JWT secret, a malformed
  DATABASE_URL, or a half-configured SMTP pair (exit 2).
- Security headers + rate limiting ship since 0.0.1 and remain
  covered by integration tests.

## Known residuals (accepted, documented)
- Live provider flows are mock-tested only; no real sandbox
  accounts were available. First production use should be
  one gateway at a time with small amounts.
- The persistence layer snapshots whole collections
  (delete+insert per changed collection, dirty-tracked since
  Wave 144); it is correct and tested against real Postgres
  but not yet a row-level incremental store.
- Plugin KV/config isolation is namespace-by-construction in
  one host process; a hostile plugin can still burn its own
  5 s/CPU budget per call — rate limits apply at the API layer.
