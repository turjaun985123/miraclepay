-- MiraclePay v0.0.1 migration 0001 core identity tables

CREATE TABLE users (
    id TEXT PRIMARY KEY,
    name TEXT,
    email TEXT UNIQUE,
    password_hash TEXT,
    role TEXT,
    email_verified BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE merchants (
    id TEXT PRIMARY KEY,
    owner_user_id TEXT REFERENCES users(id),
    business_name TEXT,
    email TEXT,
    phone TEXT,
    country TEXT,
    status TEXT DEFAULT 'pending',
    webhook_url TEXT,
    webhook_secret TEXT,
    created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY,
    merchant_id TEXT REFERENCES merchants(id),
    name TEXT,
    key_prefix TEXT,
    key_hash TEXT UNIQUE,
    scopes TEXT[] DEFAULT '{}',
    last_used_at TIMESTAMPTZ,
    revoked BOOLEAN DEFAULT false,
    created_at TIMESTAMPTZ DEFAULT now()
);

CREATE TABLE gateway_configs (
    id BIGSERIAL PRIMARY KEY,
    merchant_id TEXT REFERENCES merchants(id),
    gateway TEXT,
    merchant_number TEXT,
    api_key TEXT,
    api_secret TEXT,
    extra JSONB DEFAULT '{}',
    sandbox BOOLEAN DEFAULT false,
    enabled BOOLEAN DEFAULT true,
    UNIQUE (merchant_id, gateway)
);

CREATE TABLE platform_settings (
    key TEXT PRIMARY KEY,
    value JSONB
);

CREATE INDEX idx_merchants_owner_user_id ON merchants (owner_user_id);
CREATE INDEX idx_api_keys_merchant_id ON api_keys (merchant_id);
