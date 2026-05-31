CREATE TABLE api_keys (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    key_hash        TEXT NOT NULL UNIQUE,
    key_prefix      TEXT NOT NULL,
    name            TEXT,
    tpm_limit       INTEGER,
    rpm_limit       INTEGER,
    total_spend     DOUBLE PRECISION NOT NULL DEFAULT 0,
    is_active       BOOLEAN NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_used_at    TIMESTAMPTZ
);

CREATE TABLE spend_logs (
    id              BIGSERIAL PRIMARY KEY,
    key_id          UUID REFERENCES api_keys(id),
    request_id      TEXT NOT NULL UNIQUE,
    model           TEXT NOT NULL,
    provider        TEXT NOT NULL,
    prompt_tokens   INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    cost            DOUBLE PRECISION NOT NULL DEFAULT 0,
    duration_ms     INTEGER,
    status          TEXT NOT NULL DEFAULT 'success',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_spend_logs_key ON spend_logs(key_id);
CREATE INDEX idx_spend_logs_created ON spend_logs USING BRIN (created_at);
CREATE INDEX idx_spend_logs_request ON spend_logs(request_id);
