# Live gateway calls (v0.0.2, Waves 76–85)

Waves 7/9/10/30 shaped provider requests; v0.0.2 sends them.
`crates/gateways/src/transport.rs` holds the flows; all of them
share three rules:

1. **Credentials only from the environment.** Nothing is read from
   the database or request bodies.
2. **Sandbox by default.** Live/production endpoints require the
   provider's explicit opt-in variable (see matrix).
3. **Never fake success.** Missing credentials → `NotConfigured`
   before any network call; a non-2xx provider response →
   `Provider` error with status + body excerpt.

## Matrix

| Gateway | Flow | Required env | Sandbox default | Live opt-in |
|---|---|---|---|---|
| bKash tokenized | grant token → create → execute | `BKASH_USERNAME`, `BKASH_PASSWORD`, `BKASH_APP_KEY`, `BKASH_APP_SECRET` | `tokenized.sandbox.bka.sh` | `BKASH_SANDBOX=0` |
| Stripe | Checkout Session create | `STRIPE_SECRET_KEY` (`sk_test_…` or `sk_live_…`) | test keys hit the test API | use an `sk_live_` key |
| PayPal | OAuth → Orders create | `PAYPAL_CLIENT_ID`, `PAYPAL_CLIENT_SECRET` | `api-m.sandbox.paypal.com` | `PAYPAL_SANDBOX=0` |
| Razorpay | Orders create | `RAZORPAY_KEY_ID` (`rzp_test_…`/`rzp_live_…`), `RAZORPAY_KEY_SECRET` | `rzp_test_` keys | use an `rzp_live_` key |
| Nagad | Payment verify (GET) | `NAGAD_API_BASE`, `NAGAD_MERCHANT_ID` | none — base URL is merchant-issued, never guessed | set the live base |

## Inbound receivers

| Endpoint | Proof required before settling |
|---|---|
| `POST /api/v1/webhooks/stripe` | `Stripe-Signature` HMAC vs `STRIPE_WEBHOOK_SECRET`, 300 s window; unconfigured → 503 |
| `POST /api/v1/webhooks/paypal` | PayPal `verify-webhook-signature` API verdict `SUCCESS`; needs `PAYPAL_WEBHOOK_ID` + credentials; unconfigured → 503 |
| `POST /api/v1/callbacks/bkash` | bKash execute call returns `transactionStatus: Completed`; without credentials the callback is recorded, answered `verified: false`, and **nothing settles** |

Settling is by the payment `reference` (Stripe
`client_reference_id` / PayPal `custom_id` / bKash
`merchantInvoiceNumber`), is idempotent against provider retries,
and emits the merchant `payment.completed` webhook + audit entry,
exactly like the SMS verification path.

## Timeouts / retries

The production transport uses a 15 s timeout. Create-order flows
carry the payment reference as the provider idempotency key
(PayPal `PayPal-Request-Id`, Razorpay `receipt`, Stripe metadata),
so a caller-side retry cannot double-charge.
