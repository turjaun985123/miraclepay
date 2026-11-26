# Error catalogue

Every error response from the MiraclePay API uses one envelope
(Wave 45):

```json
{ "error": "human-readable message", "code": "stable_code", "status": 400 }
```

- `error` — human-readable; may be reworded between releases.
  **Do not parse it.**
- `code` — stable machine-readable code; never changes within
  API v1. Branch on this.
- `status` — the HTTP status, repeated in the body for clients
  that only see the payload.

The JS SDK surfaces the code as `MiraclePayError.code`.

| Code | HTTP | Meaning | Retry? |
| --- | --- | --- | --- |
| `bad_request` | 400 | Malformed request or failed validation; the message names every problem found. | Fix the request first |
| `unauthorized` | 401 | Missing/invalid/expired credentials, or a device key the server does not accept. | Re-authenticate |
| `totp_required` | 401 | Login needs a TOTP code (account has 2FA on); resubmit with `totp_code`. | Resubmit with code |
| `totp_invalid` | 401 | The supplied TOTP code did not verify. | Resubmit with a fresh code |
| `forbidden` | 403 | Authenticated, but not allowed (e.g. non-admin on an admin endpoint, non-owner write). | No |
| `not_found` | 404 | The resource does not exist — or is not visible to you (owner-scoped reads answer 404, not 403, to avoid leaking existence). | No |
| `conflict` | 409 | The request conflicts with current state (e.g. invoice not in a payable state, duplicate open dispute, SMTP not configured on dispatch). | After the state changes |
| `rate_limited` | 429 | Too many requests in the current minute window for this class (auth 10/min, SMS ingest 120/min, API 600/min). Body also carries `message` and `class`. | After the window resets |
| `unsupported_api_version` | 404 | The path names an API version this deployment does not serve; body lists `supported` versions. | Use a supported version |
| `internal_error` | 500 | Unexpected server failure. The real detail is logged server-side only, never returned. Quote the `x-request-id` response header when reporting it. | With backoff |

Notes:

- The fallback 404 for unmatched non-versioned paths is
  `{ "error": "not_found" }` (no envelope fields) — it is a
  routing miss, not an application error.
- Webhook *delivery* failures are not API errors; they follow the
  retry/dead-letter policy in `docs/webhooks.md`.
