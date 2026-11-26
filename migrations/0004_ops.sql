-- MiraclePay v0.0.1 -- Migration 0004: Operations
-- Webhook events and deliveries, notifications, audit logging,
-- inbound SMS messages, and used transaction-ID tracking.
-- Clean-room original DDL for PostgreSQL.

-- ---------------------------------------------------------------------------
-- Webhook events
-- ---------------------------------------------------------------------------

CREATE TABLE webhook_events (
    id          TEXT PRIMARY KEY,
    merchant_id TEXT REFERENCES merchants (id),
    event_type  TEXT,
    payload     JSONB,
    created_at  TIMESTAMPTZ DEFAULT now()
);

-- ---------------------------------------------------------------------------
-- Webhook deliveries
-- ---------------------------------------------------------------------------

CREATE TABLE webhook_deliveries (
    id               TEXT PRIMARY KEY,
    event_id         TEXT REFERENCES webhook_events (id),
    url              TEXT,
    status           TEXT DEFAULT 'pending',
    attempts         INT DEFAULT 0,
    last_status_code INT,
    next_retry_at    TIMESTAMPTZ,
    created_at       TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX idx_webhook_deliveries_status_next_retry_at
    ON webhook_deliveries (status, next_retry_at);

-- ---------------------------------------------------------------------------
-- Notifications
-- ---------------------------------------------------------------------------

CREATE TABLE notifications (
    id          TEXT PRIMARY KEY,
    merchant_id TEXT,
    channel     TEXT,
    recipient   TEXT,
    subject     TEXT,
    body        TEXT,
    status      TEXT DEFAULT 'queued',
    created_at  TIMESTAMPTZ
);

-- ---------------------------------------------------------------------------
-- Audit log
-- ---------------------------------------------------------------------------

CREATE TABLE audit_log (
    id            TEXT PRIMARY KEY,
    actor_user_id TEXT,
    merchant_id   TEXT,
    action        TEXT,
    entity_type   TEXT,
    entity_id     TEXT,
    ip_address    TEXT,
    user_agent    TEXT,
    details       JSONB DEFAULT '{}',
    created_at    TIMESTAMPTZ
);

CREATE INDEX idx_audit_log_merchant_id_created_at
    ON audit_log (merchant_id, created_at);

-- ---------------------------------------------------------------------------
-- SMS messages
-- ---------------------------------------------------------------------------

CREATE TABLE sms_messages (
    id                      BIGSERIAL PRIMARY KEY,
    device_id               TEXT,
    merchant_id             TEXT,
    sender                  TEXT,
    body                    TEXT,
    parsed_gateway          TEXT,
    parsed_amount_minor     BIGINT,
    parsed_trx_id           TEXT,
    match_status            TEXT DEFAULT 'unmatched',
    matched_transaction_id  TEXT,
    received_at             TIMESTAMPTZ,
    created_at              TIMESTAMPTZ DEFAULT now()
);

CREATE INDEX idx_sms_messages_merchant_id_match_status
    ON sms_messages (merchant_id, match_status);

CREATE INDEX idx_sms_messages_parsed_trx_id
    ON sms_messages (parsed_trx_id);

-- ---------------------------------------------------------------------------
-- Used transaction IDs
-- ---------------------------------------------------------------------------

CREATE TABLE used_trx_ids (
    trx_id         TEXT PRIMARY KEY,
    transaction_id TEXT,
    created_at     TIMESTAMPTZ DEFAULT now()
);
