-- MiraclePay v0.0.1 -- migration 0002: payments
-- Original clean-room DDL: transactions, payment links, invoices,
-- and checkout sessions.

CREATE TABLE IF NOT EXISTS transactions (
    id               TEXT PRIMARY KEY,
    merchant_id      TEXT REFERENCES merchants(id),
    gateway          TEXT,
    amount_minor     BIGINT,
    currency         TEXT,
    status           TEXT DEFAULT 'pending',
    customer_name    TEXT,
    customer_email   TEXT,
    customer_phone   TEXT,
    provider_trx_id  TEXT,
    sender_number    TEXT,
    reference        TEXT,
    metadata         JSONB DEFAULT '{}',
    created_at       TIMESTAMPTZ DEFAULT now(),
    updated_at       TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE IF NOT EXISTS payment_links (
    id             TEXT PRIMARY KEY,
    merchant_id    TEXT REFERENCES merchants(id),
    slug           TEXT UNIQUE,
    title          TEXT,
    description    TEXT,
    amount_minor   BIGINT,
    currency       TEXT,
    collect_phone  BOOLEAN DEFAULT false,
    collect_email  BOOLEAN DEFAULT false,
    max_uses       INT,
    uses           INT DEFAULT 0,
    active         BOOLEAN DEFAULT true,
    expires_at     TIMESTAMPTZ,
    created_at     TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE IF NOT EXISTS invoices (
    id                   TEXT PRIMARY KEY,
    merchant_id          TEXT REFERENCES merchants(id),
    number               TEXT,
    customer_name        TEXT,
    customer_email       TEXT,
    items                JSONB DEFAULT '[]',
    currency             TEXT,
    tax_bps              INT DEFAULT 0,
    discount_minor       BIGINT DEFAULT 0,
    status               TEXT DEFAULT 'draft',
    due_at               TIMESTAMPTZ,
    paid_transaction_id  TEXT,
    created_at           TIMESTAMPTZ DEFAULT now(),
    UNIQUE (merchant_id, number)
);

CREATE TABLE IF NOT EXISTS checkout_sessions (
    id             TEXT PRIMARY KEY,
    transaction_id TEXT REFERENCES transactions(id),
    expires_at     TIMESTAMPTZ,
    created_at     TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_transactions_merchant_status
    ON transactions (merchant_id, status);

CREATE INDEX IF NOT EXISTS idx_transactions_provider_trx_id
    ON transactions (provider_trx_id);

CREATE INDEX IF NOT EXISTS idx_payment_links_merchant
    ON payment_links (merchant_id);

CREATE INDEX IF NOT EXISTS idx_invoices_merchant_status
    ON invoices (merchant_id, status);
