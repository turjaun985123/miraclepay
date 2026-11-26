-- Wave 144: index the record store by collection for hydrate
-- and per-collection maintenance queries.
CREATE INDEX IF NOT EXISTS idx_mp_records_collection ON mp_records (collection);
