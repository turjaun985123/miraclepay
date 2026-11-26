# @miraclepay/sdk

JavaScript SDK for the [MiraclePay](../../README.md) payment platform
API (v0.0.1). Zero dependencies; Node 18+ or any modern browser
(global `fetch`).

## Install

```bash
npm install @miraclepay/sdk
```

(Until published, use it straight from this repo:
`npm install ./sdk/js`.)

## Usage

```js
import { MiraclePayClient } from '@miraclepay/sdk';

const client = new MiraclePayClient({
  baseUrl: 'https://pay.example.com',
  apiKey: process.env.MIRACLEPAY_API_KEY, // or { token } after login
});

const payment = await client.createPayment({
  merchant_id: 'mch_…',
  gateway: 'bkash',
  amount_minor: 125000, // ৳1,250.00
  currency: 'BDT',
});

const transactions = await client.listTransactions({ merchant_id: 'mch_…', page: 1 });
```

Errors are `MiraclePayError` with `.status` and the server's message.
The full API surface is described by the server's own OpenAPI
document at `GET /openapi.json` (also `client.openApiSpec()`).

## Test

```bash
npm test   # node --test, stubbed fetch — no server needed
```
