/** Type definitions for @miraclepay/sdk 0.0.1. */

export class MiraclePayError extends Error {
  status: number;
  body: unknown;
  constructor(status: number, message: string, body?: unknown);
}

export interface ClientOptions {
  baseUrl: string;
  token?: string;
  apiKey?: string;
  fetch?: typeof fetch;
}

export interface Money {
  amount_minor: number;
  currency: string;
}

export interface CreatePaymentInput {
  merchant_id: string;
  gateway: string;
  amount_minor: number;
  currency?: string;
  customer_email?: string;
  customer_phone?: string;
}

export class MiraclePayClient {
  constructor(options: ClientOptions);
  baseUrl: string;
  token: string | null;
  apiKey: string | null;

  request(method: string, path: string, body?: object): Promise<any>;

  health(): Promise<any>;
  openApiSpec(): Promise<any>;
  login(email: string, password: string): Promise<any>;
  me(): Promise<any>;

  createPayment(input: CreatePaymentInput): Promise<any>;
  listPayments(): Promise<any[]>;
  getPayment(id: string): Promise<any>;

  listTransactions(query?: Record<string, string | number>): Promise<any[]>;
  createRefund(transactionId: string, input: { amount_minor?: number }): Promise<any>;
  settleRefund(refundId: string, outcome: 'succeeded' | 'failed'): Promise<any>;

  createPaymentLink(input: Record<string, unknown>): Promise<any>;
  listPaymentLinks(query?: Record<string, string | number>): Promise<any[]>;

  createInvoice(input: Record<string, unknown>): Promise<any>;
  getInvoice(id: string): Promise<any>;
  invoicePdfUrl(id: string): string;

  listSettlements(query?: Record<string, string | number>): Promise<any[]>;
}

export default MiraclePayClient;
