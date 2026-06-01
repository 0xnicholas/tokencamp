-- migrations/20260531_credentials.up.sql
CREATE TABLE IF NOT EXISTS credentials (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider        TEXT NOT NULL,
    credential_type TEXT NOT NULL DEFAULT "api_key",
    encrypted_value BYTEA NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE(provider, credential_type)
);
