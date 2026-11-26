-- MiraclePay v0.0.2 migration 0005 — record store for the
-- persistence layer (Waves 53–75).
--
-- The API's collections are persisted as JSONB records, one row per
-- record, keyed by (collection, id). This sits alongside the
-- relational schema of 0001–0004 (which remains the reporting/BI
-- shape); the record store is the API's store of record when
-- DATABASE_URL is set. Snapshots replace a collection's rows
-- atomically inside one transaction, so a crash mid-snapshot can
-- never leave a half-written collection behind.

CREATE TABLE IF NOT EXISTS mp_records (
    collection TEXT NOT NULL,
    id TEXT NOT NULL,
    data JSONB NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (collection, id)
);

CREATE INDEX IF NOT EXISTS idx_mp_records_collection ON mp_records (collection);
