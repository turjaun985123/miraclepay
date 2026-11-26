import assert from 'node:assert/strict';
import { test } from 'node:test';

import { MiraclePayClient, MiraclePayError } from '../src/index.js';

/** A fetch stub that records calls and replays a canned response. */
function stubFetch(status, jsonBody) {
  const calls = [];
  const impl = async (url, init) => {
    calls.push({ url, init });
    return {
      ok: status >= 200 && status < 300,
      status,
      text: async () => (jsonBody === undefined ? '' : JSON.stringify(jsonBody)),
    };
  };
  return { impl, calls };
}

test('createPayment posts to /api/v1/payments with bearer token', async () => {
  const { impl, calls } = stubFetch(201, { id: 'pay_1' });
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test/', token: 'jwt-1', fetch: impl });
  const result = await client.createPayment({ merchant_id: 'm-1', gateway: 'bkash', amount_minor: 1000 });
  assert.equal(result.id, 'pay_1');
  assert.equal(calls.length, 1);
  assert.equal(calls[0].url, 'https://pay.test/api/v1/payments');
  assert.equal(calls[0].init.method, 'POST');
  assert.equal(calls[0].init.headers.authorization, 'Bearer jwt-1');
  assert.deepEqual(JSON.parse(calls[0].init.body), {
    merchant_id: 'm-1',
    gateway: 'bkash',
    amount_minor: 1000,
  });
});

test('apiKey is used when no token is set; login stores the token', async () => {
  const { impl, calls } = stubFetch(200, { token: 'jwt-2' });
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test', apiKey: 'mp_key', fetch: impl });
  await client.me().catch(() => {});
  assert.equal(calls[0].init.headers.authorization, 'Bearer mp_key');
  await client.login('a@b.c', 'pw');
  assert.equal(client.token, 'jwt-2');
});

test('non-2xx responses raise MiraclePayError with the server message', async () => {
  const { impl } = stubFetch(422, { error: { code: 'validation', message: 'amount too small' } });
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test', token: 't', fetch: impl });
  await assert.rejects(
    () => client.createPayment({ merchant_id: 'm', gateway: 'bkash', amount_minor: 1 }),
    (error) => {
      assert.ok(error instanceof MiraclePayError);
      assert.equal(error.status, 422);
      assert.equal(error.message, 'amount too small');
      return true;
    },
  );
});

test('the Wave 45 error envelope exposes a stable code', async () => {
  const { impl } = stubFetch(409, {
    error: 'merchant already exists',
    code: 'conflict',
    status: 409,
  });
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test', token: 't', fetch: impl });
  await assert.rejects(
    () => client.createPayment({ merchant_id: 'm', gateway: 'bkash', amount_minor: 100 }),
    (error) => {
      assert.ok(error instanceof MiraclePayError);
      assert.equal(error.status, 409);
      assert.equal(error.code, 'conflict');
      assert.equal(error.message, 'merchant already exists');
      return true;
    },
  );
});

test('listTransactions encodes the query string', async () => {
  const { impl, calls } = stubFetch(200, []);
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test', token: 't', fetch: impl });
  await client.listTransactions({ merchant_id: 'm-1', page: 2, per_page: 50 });
  assert.equal(
    calls[0].url,
    'https://pay.test/api/v1/transactions?merchant_id=m-1&page=2&per_page=50',
  );
});

test('invoicePdfUrl points at the PDF endpoint', () => {
  const client = new MiraclePayClient({ baseUrl: 'https://pay.test', token: 't', fetch: async () => { throw new Error('unused'); } });
  assert.equal(client.invoicePdfUrl('inv/1'), 'https://pay.test/api/v1/invoices/inv%2F1/pdf');
});

test('constructor requires a baseUrl', () => {
  assert.throws(() => new MiraclePayClient({}), TypeError);
});

// v0.0.2 additions (Wave 141): the new methods hit the right paths.
test('v0.0.2 methods call the documented paths', async () => {
  const { impl, calls } = stubFetch(200, { ok: true });
  const client = new MiraclePayClient({ baseUrl: 'http://api.test', token: 't', fetch: impl });
  await client.verifyTransaction('txn 1');
  await client.onboarding('mch_1');
  await client.search('mch_1', 'abc');
  await client.listPlugins();
  await client.invokePlugin('manual-bank-transfer', 'confirm_payment', { provider_ref: 'MBT-1' });
  const paths = calls.map((c) => `${c.init.method} ${c.url}`);
  assert.ok(paths[0].endsWith('/api/v1/transactions/txn%201/verify'), paths[0]);
  assert.ok(paths[1].endsWith('/api/v1/merchants/mch_1/onboarding'), paths[1]);
  assert.ok(paths[2].includes('/api/v1/search?'), paths[2]);
  assert.ok(paths[3].endsWith('/api/v1/admin/plugins'), paths[3]);
  assert.ok(paths[4].endsWith('/api/v1/admin/plugins/manual-bank-transfer/invoke'), paths[4]);
  assert.equal(
    client.receiptPdfUrl('txn_9'),
    'http://api.test/api/v1/transactions/txn_9/receipt.pdf',
  );
});
