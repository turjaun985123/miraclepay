-- MiraclePay v0.0.1
-- Migration 0003: Billing
-- Plans, subscriptions, refunds, and settlements.

CREATE TABLE IF NOT EXISTS plans (
    id          TEXT PRIMARY KEY,
    merchant_id TEXT REFERENCES merchants(id),
    name        TEXT NOT NULL,
    amount_minor BIGINT NOT NULL,
    currency    TEXT NOT NULL,
    interval    TEXT NOT NULL,
    trial_days  INT NOT NULL DEFAULT 0,
    active      BOOLEAN NOT NULL DEFAULT true,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS subscriptions (
    id                    TEXT PRIMARY KEY,
    plan_id               TEXT REFERENCES plans(id),
    merchant_id           TEXT REFERENCES merchants(id),
    customer_email        TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'trialing',
    current_period_end    TIMESTAMPTZ,
    cancel_at_period_end  BOOLEAN NOT NULL DEFAULT false,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS refunds (
    id                  TEXT PRIMARY KEY,
    transaction_id      TEXT REFERENCES transactions(id),
    merchant_id         TEXT REFERENCES merchants(id),
    amount_minor        BIGINT NOT NULL,
    currency            TEXT NOT NULL,
    reason              TEXT,
    status              TEXT NOT NULL DEFAULT 'pending',
    provider_refund_id  TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS settlements (
    id               TEXT PRIMARY KEY,
    merchant_id      TEXT REFERENCES merchants(id),
    gross_minor      BIGINT NOT NULL,
    fee_minor        BIGINT NOT NULL,
    net_minor        BIGINT NOT NULL,
    currency         TEXT NOT NULL,
    status           TEXT NOT NULL DEFAULT 'pending',
    period_start     TIMESTAMPTZ,
    period_end       TIMESTAMPTZ,
    payout_method    TEXT,
    payout_reference TEXT,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_subscriptions_merchant_id_status
    ON subscriptions (merchant_id, status);

CREATE INDEX IF NOT EXISTS idx_refunds_transaction_id
    ON refunds (transaction_id);

CREATE INDEX IF NOT EXISTS idx_settlements_merchant_id_status
    ON settlements (merchant_id, status);
