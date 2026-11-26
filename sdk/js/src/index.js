/**
 * MiraclePay JavaScript SDK (v0.0.1).
 *
 * A thin, zero-dependency client for the MiraclePay REST API.
 * Every method maps to exactly one API operation — the same
 * one-operation-one-endpoint rule the server follows.
 *
 * Auth: pass either `token` (a JWT from POST /api/v1/auth/login)
 * or `apiKey` (sent as a bearer token; API keys authenticate the
 * merchant-scoped endpoints).
 */

/** Error thrown for non-2xx API responses. */
export class MiraclePayError extends Error {
  /**
   * @param {number} status HTTP status code.
   * @param {string} message Server error message (or status text).
   * @param {unknown} [body] Parsed response body, when available.
   */
  constructor(status, message, body) {
    super(message);
    this.name = 'MiraclePayError';
    this.status = status;
    this.body = body;
    /**
     * Stable machine-readable error code from the API's error
     * envelope (Wave 45; see docs/errors.md). `undefined` for
     * responses that predate the envelope.
     * @type {string | undefined}
     */
    this.code =
      body && typeof body === 'object' && typeof body.code === 'string'
        ? body.code
        : undefined;
  }
}

export class MiraclePayClient {
  /**
   * @param {object} options
   * @param {string} options.baseUrl API origin, e.g. "https://pay.example.com".
   * @param {string} [options.token] JWT access token.
   * @param {string} [options.apiKey] Merchant API key (used when no token).
   * @param {typeof fetch} [options.fetch] Custom fetch implementation (tests).
   */
  constructor({ baseUrl, token, apiKey, fetch: fetchImpl } = {}) {
    if (!baseUrl) throw new TypeError('MiraclePayClient: baseUrl is required');
    this.baseUrl = baseUrl.replace(/\/+$/, '');
    this.token = token ?? null;
    this.apiKey = apiKey ?? null;
    this.fetch = fetchImpl ?? globalThis.fetch;
    if (typeof this.fetch !== 'function') {
      throw new TypeError('MiraclePayClient: no fetch implementation available (Node 18+ required)');
    }
  }

  /**
   * Core request helper. Returns the parsed JSON body, or `null`
   * for empty responses. Throws {@link MiraclePayError} on non-2xx.
   * @param {string} method
   * @param {string} path Path starting with `/`.
   * @param {object} [body] JSON body for POST/PATCH/PUT.
   * @returns {Promise<any>}
   */
  async request(method, path, body) {
    const headers = { accept: 'application/json' };
    const credential = this.token ?? this.apiKey;
    if (credential) headers.authorization = `Bearer ${credential}`;
    const init = { method, headers };
    if (body !== undefined) {
      headers['content-type'] = 'application/json';
      init.body = JSON.stringify(body);
    }
    const response = await this.fetch(`${this.baseUrl}${path}`, init);
    const text = await response.text();
    let parsed = null;
    if (text) {
      try {
        parsed = JSON.parse(text);
      } catch {
        parsed = text;
      }
    }
    if (!response.ok) {
      const message =
        (parsed && typeof parsed === 'object' && (parsed.error?.message ?? parsed.error)) ||
        `HTTP ${response.status}`;
      throw new MiraclePayError(response.status, String(message), parsed);
    }
    return parsed;
  }

  // -- System ------------------------------------------------------
  /** GET /health — liveness and storage mode. */
  health() {
    return this.request('GET', '/health');
  }

  /** GET /openapi.json — the OpenAPI 3.1 document. */
  openApiSpec() {
    return this.request('GET', '/openapi.json');
  }

  // -- Auth ----------------------------------------------------------
  /** POST /api/v1/auth/login — sets this client's token on success. */
  async login(email, password) {
    const result = await this.request('POST', '/api/v1/auth/login', { email, password });
    if (result && typeof result.token === 'string') this.token = result.token;
    return result;
  }

  /** GET /api/v1/auth/me */
  me() {
    return this.request('GET', '/api/v1/auth/me');
  }

  // -- Payments ------------------------------------------------------
  /** POST /api/v1/payments */
  createPayment(input) {
    return this.request('POST', '/api/v1/payments', input);
  }

  /** GET /api/v1/payments */
  listPayments() {
    return this.request('GET', '/api/v1/payments');
  }

  /** GET /api/v1/payments/:id */
  getPayment(id) {
    return this.request('GET', `/api/v1/payments/${encodeURIComponent(id)}`);
  }

  // -- Transactions & refunds ---------------------------------------
  /** GET /api/v1/transactions (pass a query object for filters/paging). */
  listTransactions(query = {}) {
    const qs = new URLSearchParams(query).toString();
    return this.request('GET', `/api/v1/transactions${qs ? `?${qs}` : ''}`);
  }

  /** POST /api/v1/transactions/:id/refunds */
  createRefund(transactionId, input) {
    return this.request(
      'POST',
      `/api/v1/transactions/${encodeURIComponent(transactionId)}/refunds`,
      input,
    );
  }

  /** POST /api/v1/refunds/:id/settle — outcome: "succeeded" | "failed". */
  settleRefund(refundId, outcome) {
    return this.request('POST', `/api/v1/refunds/${encodeURIComponent(refundId)}/settle`, {
      outcome,
    });
  }

  // -- Payment links ---------------------------------------------------
  /** POST /api/v1/payment-links */
  createPaymentLink(input) {
    return this.request('POST', '/api/v1/payment-links', input);
  }

  /** GET /api/v1/payment-links */
  listPaymentLinks(query = {}) {
    const qs = new URLSearchParams(query).toString();
    return this.request('GET', `/api/v1/payment-links${qs ? `?${qs}` : ''}`);
  }

  // -- Invoices --------------------------------------------------------
  /** POST /api/v1/invoices */
  createInvoice(input) {
    return this.request('POST', '/api/v1/invoices', input);
  }

  /** GET /api/v1/invoices/:id */
  getInvoice(id) {
    return this.request('GET', `/api/v1/invoices/${encodeURIComponent(id)}`);
  }

  /** Direct URL of the invoice PDF (GET /api/v1/invoices/:id/pdf). */
  invoicePdfUrl(id) {
    return `${this.baseUrl}/api/v1/invoices/${encodeURIComponent(id)}/pdf`;
  }

  // -- v0.0.2: auth completion -------------------------------------------
  /** POST /api/v1/auth/password-reset/request — always succeeds silently. */
  requestPasswordReset(email) {
    return this.request('POST', '/api/v1/auth/password-reset/request', { email });
  }

  /** POST /api/v1/auth/password-reset/confirm */
  confirmPasswordReset(token, newPassword) {
    return this.request('POST', '/api/v1/auth/password-reset/confirm', {
      token,
      new_password: newPassword,
    });
  }

  /** GET /api/v1/auth/sessions */
  listSessions() {
    return this.request('GET', '/api/v1/auth/sessions');
  }

  /** DELETE /api/v1/auth/sessions/:id */
  revokeSession(id) {
    return this.request('DELETE', `/api/v1/auth/sessions/${encodeURIComponent(id)}`);
  }

  // -- v0.0.2: product depth ----------------------------------------------
  /** GET /api/v1/merchants/:id/onboarding */
  onboarding(merchantId) {
    return this.request('GET', `/api/v1/merchants/${encodeURIComponent(merchantId)}/onboarding`);
  }

  /** GET /api/v1/search?q=&merchant_id= */
  search(merchantId, q) {
    const qs = new URLSearchParams({ merchant_id: merchantId, q }).toString();
    return this.request('GET', `/api/v1/search?${qs}`);
  }

  /** POST /api/v1/transactions/:id/verify — settle on gateway confirmation. */
  verifyTransaction(id) {
    return this.request('POST', `/api/v1/transactions/${encodeURIComponent(id)}/verify`);
  }

  /** GET /api/v1/payment-links/:id/stats */
  paymentLinkStats(id) {
    return this.request('GET', `/api/v1/payment-links/${encodeURIComponent(id)}/stats`);
  }

  /** Direct URL of the transactions CSV export. */
  transactionsCsvUrl(merchantId) {
    return `${this.baseUrl}/api/v1/transactions/export.csv?merchant_id=${encodeURIComponent(merchantId)}`;
  }

  /** Direct URL of a payment receipt PDF. */
  receiptPdfUrl(transactionId) {
    return `${this.baseUrl}/api/v1/transactions/${encodeURIComponent(transactionId)}/receipt.pdf`;
  }

  /** POST /api/v1/gateways/:provider/test */
  testGateway(provider, merchantId) {
    return this.request('POST', `/api/v1/gateways/${encodeURIComponent(provider)}/test`, {
      merchant_id: merchantId,
    });
  }

  // -- v0.0.2: plugins (admin) ----------------------------------------------
  /** GET /api/v1/admin/plugins */
  listPlugins() {
    return this.request('GET', '/api/v1/admin/plugins');
  }

  /** POST /api/v1/admin/plugins/:id/invoke */
  invokePlugin(id, operation, args = {}) {
    return this.request('POST', `/api/v1/admin/plugins/${encodeURIComponent(id)}/invoke`, {
      operation,
      args,
    });
  }

  // -- Settlements -------------------------------------------------------
  /** GET /api/v1/settlements */
  listSettlements(query = {}) {
    const qs = new URLSearchParams(query).toString();
    return this.request('GET', `/api/v1/settlements${qs ? `?${qs}` : ''}`);
  }
}

export default MiraclePayClient;
