/**
 * MiraclePay v0.0.1 — API client
 *
 * Clean-room, framework-free TypeScript client for the MiraclePay Rust
 * backend (Axum). All endpoints live under `/api/v1/...` on `API_URL`.
 *
 * No framework imports: this module only relies on the standard
 * `fetch`, `localStorage`, and `import.meta.env` facilities, so it can
 * be used from Astro pages, plain scripts, or any other frontend code.
 */

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

export const API_URL: string =
  import.meta.env.PUBLIC_API_URL ?? 'http://localhost:8080';

// ---------------------------------------------------------------------------
// Token storage
// ---------------------------------------------------------------------------

const TOKEN_KEY = 'mp_token';

/**
 * Read the stored auth token, or `null` when there is none.
 * Safe to call at build / SSR time, where `localStorage` does not exist.
 */
export function getToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    return localStorage.getItem(TOKEN_KEY);
  } catch {
    return null;
  }
}

/** Persist the auth token. No-op when `localStorage` is unavailable. */
export function setToken(token: string): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(TOKEN_KEY, token);
  } catch {
    // Storage may be unavailable (private mode, quota, SSR); ignore.
  }
}

/** Remove the stored auth token. No-op when `localStorage` is unavailable. */
export function clearToken(): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.removeItem(TOKEN_KEY);
    localStorage.removeItem(REFRESH_KEY);
  } catch {
    // Ignore unavailable storage.
  }
}

// Refresh tokens (Wave 25): stored beside the access token; on a
// 401 the client exchanges the refresh token once for a fresh pair
// (the server rotates it — see POST /api/v1/auth/refresh).
const REFRESH_KEY = 'mp_refresh_token';

export function getRefreshToken(): string | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    return localStorage.getItem(REFRESH_KEY);
  } catch {
    return null;
  }
}

export function setRefreshToken(token: string): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(REFRESH_KEY, token);
  } catch {
    // Ignore unavailable storage.
  }
}

// ---------------------------------------------------------------------------
// Core fetch helper
// ---------------------------------------------------------------------------

export interface ApiFetchOptions extends RequestInit {
  /**
   * Whether to attach the stored Bearer token. Defaults to `true`;
   * pass `auth: false` for public endpoints (login, register, public
   * checkout / payment-link pages, ...).
   */
  auth?: boolean;
  /** Internal: set on the single retry after a token refresh. */
  __retried?: boolean;
}

interface ApiErrorBody {
  error?: string;
  message?: string;
  [key: string]: unknown;
}

/**
 * Perform a JSON request against the MiraclePay API and return the
 * parsed response body typed as `T`.
 *
 * - Sends `Accept: application/json` always, and
 *   `Content-Type: application/json` when a body is present.
 * - Attaches `Authorization: Bearer <token>` when `auth !== false`
 *   and a token is stored.
 * - On a non-2xx response, throws an `Error` whose message is the
 *   server's `{ error }` field when present, otherwise a generic
 *   status-based message.
 * - Returns `undefined` (as `T`) for empty / 204 responses.
 */
export async function apiFetch<T>(
  path: string,
  options: ApiFetchOptions = {},
): Promise<T> {
  const { auth, headers, ...rest } = options;

  const finalHeaders = new Headers(headers ?? {});
  if (!finalHeaders.has('Accept')) {
    finalHeaders.set('Accept', 'application/json');
  }
  if (rest.body != null && !finalHeaders.has('Content-Type')) {
    finalHeaders.set('Content-Type', 'application/json');
  }

  if (auth !== false) {
    const token = getToken();
    if (token) {
      finalHeaders.set('Authorization', `Bearer ${token}`);
    }
  }

  const url = path.startsWith('http')
    ? path
    : `${API_URL}${path.startsWith('/') ? '' : '/'}${path}`;

  const response = await fetch(url, { ...rest, headers: finalHeaders });

  // Wave 25: an expired access token triggers exactly one refresh
  // exchange and one retry; auth endpoints themselves never loop.
  if (
    response.status === 401 &&
    auth !== false &&
    !options.__retried &&
    !path.includes('/auth/') &&
    getRefreshToken()
  ) {
    if (await tryRefresh()) {
      return apiFetch<T>(path, { ...options, __retried: true });
    }
  }

  if (!response.ok) {
    let message = `Request failed with status ${response.status}`;
    try {
      const body = (await response.json()) as ApiErrorBody;
      if (typeof body?.error === 'string' && body.error.length > 0) {
        message = body.error;
      } else if (typeof body?.message === 'string' && body.message.length > 0) {
        message = body.message;
      }
    } catch {
      // Non-JSON error body; keep the generic status message.
    }
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  const text = await response.text();
  if (!text) {
    return undefined as T;
  }
  return JSON.parse(text) as T;
}

/** Serialise a plain object as a JSON request body. */
function jsonBody(data: unknown): string {
  return JSON.stringify(data ?? {});
}

/** Build a query string from defined, non-empty values. */
function queryString(params: Record<string, unknown>): string {
  const search = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    if (value !== undefined && value !== null && value !== '') {
      search.set(key, String(value));
    }
  }
  const qs = search.toString();
  return qs ? `?${qs}` : '';
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export interface User {
  id: string;
  name: string;
  email: string;
  role?: string;
  is_admin?: boolean;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface AuthResponse {
  token: string;
  /** Rotating refresh token (Wave 25); present on login/refresh. */
  refresh_token?: string;
  user: User;
  [key: string]: unknown;
}

export interface Merchant {
  id: string;
  user_id?: string;
  name: string;
  slug?: string;
  email?: string;
  phone?: string;
  website?: string;
  logo_url?: string;
  currency?: string;
  status?: string;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface CreateMerchantData {
  name: string;
  email?: string;
  phone?: string;
  website?: string;
  logo_url?: string;
  currency?: string;
  [key: string]: unknown;
}

export interface MerchantBalance {
  merchant_id: string;
  currency: string;
  /** Balance in the currency's minor unit (e.g. poisha / cents). */
  available_minor: number;
  pending_minor?: number;
  total_minor?: number;
  [key: string]: unknown;
}

export interface Transaction {
  id: string;
  merchant_id?: string;
  type?: string;
  status: string;
  /** Amount in the currency's minor unit. */
  amount_minor: number;
  currency: string;
  fee_minor?: number;
  net_minor?: number;
  gateway?: string;
  method?: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  reference?: string;
  description?: string;
  metadata?: Record<string, unknown>;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface CreatePaymentData {
  merchant_id: string;
  amount_minor: number;
  currency?: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  description?: string;
  reference?: string;
  redirect_url?: string;
  cancel_url?: string;
  metadata?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface CheckoutDetails {
  id: string;
  merchant_id?: string;
  merchant_name?: string;
  merchant_logo_url?: string;
  amount_minor: number;
  currency: string;
  status: string;
  description?: string;
  gateways?: string[];
  methods?: string[];
  expires_at?: string;
  [key: string]: unknown;
}

export interface SubmitCheckoutData {
  gateway: string;
  method?: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  sender_number?: string;
  transaction_id?: string;
  [key: string]: unknown;
}

export interface PaymentLink {
  id: string;
  merchant_id?: string;
  slug: string;
  title: string;
  description?: string;
  /** Fixed amount in minor units, when the link is not open-amount. */
  amount_minor?: number | null;
  currency: string;
  status?: string;
  url?: string;
  expires_at?: string | null;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface CreateLinkData {
  merchant_id: string;
  title: string;
  description?: string;
  amount_minor?: number | null;
  currency?: string;
  slug?: string;
  /** Wave 15: cap on uses; 1 = one-time link. */
  max_uses?: number | null;
  /** Wave 15: absolute expiry (RFC 3339, future)… */
  expires_at?: string | null;
  /** …or relative expiry in hours from creation. */
  expires_in_hours?: number | null;
  [key: string]: unknown;
}

export interface PayLinkData {
  amount_minor?: number;
  gateway?: string;
  method?: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  sender_number?: string;
  transaction_id?: string;
  [key: string]: unknown;
}

export interface InvoiceItem {
  description: string;
  quantity?: number;
  unit_amount_minor: number;
  [key: string]: unknown;
}

export interface Invoice {
  id: string;
  merchant_id?: string;
  number?: string;
  status: string;
  amount_minor: number;
  currency: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  items?: InvoiceItem[];
  due_at?: string | null;
  issued_at?: string;
  paid_at?: string | null;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface CreateInvoiceData {
  merchant_id: string;
  amount_minor?: number;
  currency?: string;
  customer_name?: string;
  customer_email?: string;
  customer_phone?: string;
  items?: InvoiceItem[];
  due_at?: string | null;
  [key: string]: unknown;
}

export interface GatewayConfigView {
  id?: string;
  merchant_id?: string;
  gateway: string;
  enabled: boolean;
  /** Non-secret, display-safe configuration values. */
  config?: Record<string, unknown>;
  has_credentials?: boolean;
  created_at?: string;
  updated_at?: string;
  [key: string]: unknown;
}

export interface GatewayInfo {
  id: string;
  name: string;
  methods?: string[];
  currencies?: string[];
  [key: string]: unknown;
}

export interface UpsertGatewayData {
  gateway: string;
  enabled?: boolean;
  config?: Record<string, unknown>;
  credentials?: Record<string, unknown>;
  [key: string]: unknown;
}

export interface ApiKey {
  id: string;
  merchant_id?: string;
  name?: string;
  prefix?: string;
  last_used_at?: string | null;
  created_at?: string;
  revoked_at?: string | null;
  /** Device presence (Wave 36): revoked | never_connected | online | idle | offline. */
  device_status?: string;
  [key: string]: unknown;
}

/** Returned once, at creation time — `key` is the full secret. */
export interface ApiKeyCreated extends ApiKey {
  key: string;
  secret?: string;
}

export interface CreateApiKeyData {
  merchant_id: string;
  name?: string;
  [key: string]: unknown;
}

export interface RefundData {
  amount_minor?: number;
  reason?: string;
  [key: string]: unknown;
}

export interface ListTransactionsParams {
  merchant_id?: string;
  status?: string;
  type?: string;
  gateway?: string;
  from?: string;
  to?: string;
  limit?: number;
  offset?: number;
  page?: number;
  [key: string]: unknown;
}

/** Loose dashboard / admin statistics shape. */
export interface DashboardStats {
  total_merchants?: number;
  total_users?: number;
  total_transactions?: number;
  total_volume_minor?: number;
  successful_transactions?: number;
  pending_transactions?: number;
  failed_transactions?: number;
  refunded_transactions?: number;
  currency?: string;
  today_volume_minor?: number;
  today_transactions?: number;
  [key: string]: unknown;
}

// ---------------------------------------------------------------------------
// Auth
// ---------------------------------------------------------------------------

export async function login(
  email: string,
  password: string,
  totpCode?: string,
): Promise<AuthResponse> {
  const result = await apiFetch<AuthResponse>('/api/v1/auth/login', {
    method: 'POST',
    auth: false,
    body: jsonBody({ email, password, totp_code: totpCode }),
  });
  // Store the pair centrally (Wave 25); pages may still call
  // setToken themselves — it writes the same value.
  if (result.token) setToken(result.token);
  if (result.refresh_token) setRefreshToken(result.refresh_token);
  return result;
}

/**
 * Exchange the stored refresh token for a fresh pair (Wave 25).
 * Returns false — and clears the stored pair — when the refresh
 * token is unknown, revoked, or expired.
 */
export async function tryRefresh(): Promise<boolean> {
  const refreshToken = getRefreshToken();
  if (!refreshToken) return false;
  try {
    const result = await apiFetch<AuthResponse>('/api/v1/auth/refresh', {
      method: 'POST',
      auth: false,
      body: jsonBody({ refresh_token: refreshToken }),
    });
    if (result.token) setToken(result.token);
    if (result.refresh_token) setRefreshToken(result.refresh_token);
    return Boolean(result.token);
  } catch {
    clearToken();
    return false;
  }
}

/** Revoke the stored refresh token server-side and clear the pair. */
export async function logout(): Promise<void> {
  const refreshToken = getRefreshToken();
  if (refreshToken) {
    try {
      await apiFetch('/api/v1/auth/logout', {
        method: 'POST',
        auth: false,
        body: jsonBody({ refresh_token: refreshToken }),
      });
    } catch {
      // Server unreachable: local sign-out still proceeds.
    }
  }
  clearToken();
}

export function register(
  name: string,
  email: string,
  password: string,
): Promise<AuthResponse> {
  return apiFetch<AuthResponse>('/api/v1/auth/register', {
    method: 'POST',
    auth: false,
    body: jsonBody({ name, email, password }),
  });
}

export function me(): Promise<User> {
  return apiFetch<User>('/api/v1/auth/me');
}

// Merchant KYC (Wave 33)

export interface KycProfile {
  merchant_id: string;
  business_type?: string;
  trade_license_no?: string | null;
  nid_no?: string;
  address?: string;
  status: string;
  decision_note?: string | null;
  submitted_at?: string;
  decided_at?: string | null;
  [key: string]: unknown;
}

export function getKyc(merchantId: string): Promise<KycProfile> {
  return apiFetch<KycProfile>(`/api/v1/merchants/${encodeURIComponent(merchantId)}/kyc`);
}

export function submitKyc(
  merchantId: string,
  data: { business_type: string; trade_license_no?: string; nid_no: string; address: string },
): Promise<KycProfile> {
  return apiFetch<KycProfile>(`/api/v1/merchants/${encodeURIComponent(merchantId)}/kyc`, {
    method: 'PUT',
    body: jsonBody(data),
  });
}

export function listPendingKyc(): Promise<KycProfile[]> {
  return apiFetch<KycProfile[]>('/api/v1/admin/kyc/pending');
}

export function decideKyc(
  merchantId: string,
  decision: 'verified' | 'rejected',
  note?: string,
): Promise<KycProfile> {
  return apiFetch<KycProfile>(
    `/api/v1/admin/merchants/${encodeURIComponent(merchantId)}/kyc/decision`,
    { method: 'POST', body: jsonBody({ decision, note }) },
  );
}

// Two-factor authentication (Wave 26)

/** Start TOTP setup: returns the one-time secret + otpauth URI. */
export function totpSetup(): Promise<{ secret: string; otpauth_uri: string; enabled: boolean }> {
  return apiFetch('/api/v1/auth/2fa/setup', { method: 'POST', body: jsonBody({}) });
}

/** Enable TOTP by proving possession with a current code. */
export function totpEnable(code: string): Promise<{ enabled: boolean }> {
  return apiFetch('/api/v1/auth/2fa/enable', { method: 'POST', body: jsonBody({ code }) });
}

/** Disable TOTP with a current code; clears the server-side secret. */
export function totpDisable(code: string): Promise<{ enabled: boolean }> {
  return apiFetch('/api/v1/auth/2fa/disable', { method: 'POST', body: jsonBody({ code }) });
}

// ---------------------------------------------------------------------------
// Merchants
// ---------------------------------------------------------------------------

export function listMerchants(): Promise<Merchant[]> {
  return apiFetch<Merchant[]>('/api/v1/merchants');
}

export function createMerchant(data: CreateMerchantData): Promise<Merchant> {
  return apiFetch<Merchant>('/api/v1/merchants', {
    method: 'POST',
    body: jsonBody(data),
  });
}

export function merchantBalance(id: string): Promise<MerchantBalance> {
  return apiFetch<MerchantBalance>(
    `/api/v1/merchants/${encodeURIComponent(id)}/balance`,
  );
}

// ---------------------------------------------------------------------------
// Payments / checkout
// ---------------------------------------------------------------------------

export function listPayments(merchantId?: string): Promise<Transaction[]> {
  const qs = queryString({ merchant_id: merchantId });
  return apiFetch<Transaction[]>(`/api/v1/payments${qs}`);
}

export function createPayment(data: CreatePaymentData): Promise<Transaction> {
  return apiFetch<Transaction>('/api/v1/payments', {
    method: 'POST',
    body: jsonBody(data),
  });
}

export function checkoutDetails(id: string): Promise<CheckoutDetails> {
  return apiFetch<CheckoutDetails>(
    `/api/v1/checkout/${encodeURIComponent(id)}`,
    { auth: false },
  );
}

export function submitCheckout(
  id: string,
  data: SubmitCheckoutData,
): Promise<Transaction> {
  return apiFetch<Transaction>(
    `/api/v1/checkout/${encodeURIComponent(id)}/submit`,
    {
      method: 'POST',
      auth: false,
      body: jsonBody(data),
    },
  );
}

// ---------------------------------------------------------------------------
// Payment links
// ---------------------------------------------------------------------------

export function listLinks(merchantId?: string): Promise<PaymentLink[]> {
  const qs = queryString({ merchant_id: merchantId });
  return apiFetch<PaymentLink[]>(`/api/v1/payment-links${qs}`);
}

export function createLink(data: CreateLinkData): Promise<PaymentLink> {
  return apiFetch<PaymentLink>('/api/v1/payment-links', {
    method: 'POST',
    body: jsonBody(data),
  });
}

export function publicLink(slug: string): Promise<PaymentLink> {
  return apiFetch<PaymentLink>(
    `/api/v1/payment-links/public/${encodeURIComponent(slug)}`,
    { auth: false },
  );
}

export function payLink(slug: string, data: PayLinkData): Promise<Transaction> {
  return apiFetch<Transaction>(
    `/api/v1/payment-links/public/${encodeURIComponent(slug)}/pay`,
    {
      method: 'POST',
      auth: false,
      body: jsonBody(data),
    },
  );
}

// ---------------------------------------------------------------------------
// Invoices
// ---------------------------------------------------------------------------

export function listInvoices(merchantId?: string): Promise<Invoice[]> {
  const qs = queryString({ merchant_id: merchantId });
  return apiFetch<Invoice[]>(`/api/v1/invoices${qs}`);
}

export function createInvoice(data: CreateInvoiceData): Promise<Invoice> {
  return apiFetch<Invoice>('/api/v1/invoices', {
    method: 'POST',
    body: jsonBody(data),
  });
}

export function sendInvoice(id: string): Promise<Invoice> {
  return apiFetch<Invoice>(
    `/api/v1/invoices/${encodeURIComponent(id)}/send`,
    { method: 'POST', body: jsonBody({}) },
  );
}

// ---------------------------------------------------------------------------
// Transactions
// ---------------------------------------------------------------------------

export function listTransactions(
  params: ListTransactionsParams = {},
): Promise<Transaction[]> {
  const qs = queryString(params);
  return apiFetch<Transaction[]>(`/api/v1/transactions${qs}`);
}

export function refund(txId: string, data: RefundData = {}): Promise<Transaction> {
  return apiFetch<Transaction>(
    `/api/v1/transactions/${encodeURIComponent(txId)}/refund`,
    { method: 'POST', body: jsonBody(data) },
  );
}

// ---------------------------------------------------------------------------
// Gateways
// ---------------------------------------------------------------------------

export function listGateways(): Promise<GatewayInfo[]> {
  return apiFetch<GatewayInfo[]>('/api/v1/gateways', { auth: false });
}

export function merchantGateways(
  merchantId: string,
): Promise<GatewayConfigView[]> {
  return apiFetch<GatewayConfigView[]>(
    `/api/v1/merchants/${encodeURIComponent(merchantId)}/gateways`,
  );
}

export function upsertGateway(
  merchantId: string,
  data: UpsertGatewayData,
): Promise<GatewayConfigView> {
  return apiFetch<GatewayConfigView>(
    `/api/v1/merchants/${encodeURIComponent(merchantId)}/gateways`,
    { method: 'PUT', body: jsonBody(data) },
  );
}

// ---------------------------------------------------------------------------
// API keys
// ---------------------------------------------------------------------------

export function listApiKeys(merchantId: string): Promise<ApiKey[]> {
  return apiFetch<ApiKey[]>(
    `/api/v1/merchants/${encodeURIComponent(merchantId)}/api-keys`,
  );
}

export function createApiKey(data: CreateApiKeyData): Promise<ApiKeyCreated> {
  return apiFetch<ApiKeyCreated>(
    `/api/v1/merchants/${encodeURIComponent(data.merchant_id)}/api-keys`,
    { method: 'POST', body: jsonBody(data) },
  );
}

export function revokeApiKey(id: string): Promise<void> {
  return apiFetch<void>(`/api/v1/api-keys/${encodeURIComponent(id)}/revoke`, {
    method: 'POST',
  });
}

// ---------------------------------------------------------------------------
// Admin
// ---------------------------------------------------------------------------

export function adminStats(): Promise<DashboardStats> {
  return apiFetch<DashboardStats>('/api/v1/admin/stats');
}

/** One audit-log entry, mirroring the domain `AuditEntry` shape. */
export interface AuditEntry {
  id: string;
  actor_user_id: string | null;
  merchant_id: string | null;
  action: string;
  entity_type: string;
  entity_id: string | null;
  ip_address: string | null;
  user_agent: string | null;
  details: unknown;
  created_at: string;
}

export interface ListAuditParams {
  limit?: number;
  action?: string;
  entity_type?: string;
  actor_user_id?: string;
  page?: number;
  per_page?: number;
}

/** Admin audit log (Wave 13): newest first, filterable + pageable. */
export function listAudit(params: ListAuditParams = {}): Promise<AuditEntry[]> {
  const qs = queryString(params);
  return apiFetch<AuditEntry[]>(`/api/v1/admin/audit${qs}`);
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

/**
 * Format an amount given in minor units (e.g. poisha / cents) as a
 * human-readable string with a currency code, thousands separators,
 * and exactly two decimal places.
 *
 * @example formatMoney(123456, 'BDT') // "BDT 1,234.56"
 */
export function formatMoney(amountMinor: number, currency: string): string {
  const value = (Number(amountMinor) || 0) / 100;
  const formatted = value.toLocaleString('en-US', {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  const code = (currency ?? '').trim().toUpperCase();
  return code ? `${code} ${formatted}` : formatted;
}
