CREATE TABLE IF NOT EXISTS signing_keyring_versions (
    version BIGINT PRIMARY KEY,
    manifest JSONB NOT NULL,
    activated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
