# Webhooks

MiraclePay v0.0.1 — how webhook delivery, signing, and events work
(Wave 32).

## Delivery

When something happens to a merchant's resources, the API records a
`WebhookEvent`; the worker delivers it by HTTPS POST to the
merchant's configured webhook URL with a JSON body (the full event
object, including `payload`).

Headers sent with every delivery:

| Header | Value |
|---|---|
| `content-type` | `application/json` |
| `x-miraclepay-event` | the event type, e.g. `payment.completed` |
| `x-miraclepay-delivery` | the delivery attempt's id |
| `x-miraclepay-signature` | **v1** signature: hex HMAC-SHA256 of the raw body |
| `x-miraclepay-signature-v2` | **v2** signature: `t=<unix>,v2=<hex>` (see below) |

Retries: up to 8 attempts with exponential backoff starting at
60 s; after the last failure the delivery is dead-lettered (visible
in the dashboard's webhook view and via `GET /api/v1/webhooks/deliveries`).
Any 2xx response counts as delivered.

## Verifying signatures

Always verify against the **raw request body bytes**, never a
re-serialised parse.

### v2 (recommended — replay-resistant)

The signed content is `"{t}.{raw body}"`. Verify:

1. Parse `t` and `v2` from the `x-miraclepay-signature-v2` header.
2. Reject when `t` is more than **300 seconds** from your clock
   (either direction) — this is what stops replayed deliveries.
3. Compute HMAC-SHA256 with your merchant webhook secret over
   `"{t}.{raw body}"` and compare with `v2` in constant time.

Reference implementation: `miraclepay_domain::webhook::verify_signature_v2`.

### v1 (legacy)

`x-miraclepay-signature` is the hex HMAC-SHA256 of the raw body
(`webhook::verify_signature`). It has no timestamp, so a captured
delivery can be replayed indefinitely — prefer v2. Both headers
are sent during the v0.0.1 transition period.

## Event catalogue

The canonical list lives in code
(`miraclepay_domain::webhook::EVENT_TYPES`); payloads carry the
ids and headline fields of the affected resource.

| Event | When | Payload highlights |
|---|---|---|
| `payment.created` | A payment is initiated | transaction_id, gateway, amount_minor, currency, status, reference |
| `payment.completed` | An SMS verification settles a payment | transaction_id, amount_minor, trx_id |
| `payment.refunded` | A transaction's refund coverage changes its status | transaction_id, refunds_cover_full_amount |
| `refund.succeeded` | A refund is settled as succeeded | refund_id, transaction_id, amount_minor |
| `refund.failed` | A refund is settled as failed | refund_id, transaction_id |
| `subscription.created` | A subscription starts | subscription_id, plan_id, status, customer_email |
| `subscription.cancelled` | A subscription is cancelled | subscription_id, status |
| `subscription.paused` | A subscription is set to end at period end | subscription_id, status |
| `subscription.resumed` | A paused subscription resumes | subscription_id, status |
| `settlement.paid` | A settlement payout is recorded | settlement_id, gross_minor, fee_minor, net_minor, currency, payout_reference |

Events added in Wave 32: the four `subscription.*` events and
`settlement.paid` — before this wave those actions were only
audit-logged and never reached merchants' endpoints.

## Testing

`POST /api/v1/webhooks/test-signature` verifies a signature against
a payload with your secret, so an integration can be checked
without a live delivery.
