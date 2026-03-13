-- 025_ssf.sql

-- SSF Streams
CREATE TABLE IF NOT EXISTS ssf_streams (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id TEXT NOT NULL REFERENCES tenants(id),
    receiver_url TEXT,
    delivery_method TEXT NOT NULL DEFAULT 'push',
    event_types TEXT[] NOT NULL DEFAULT '{}',
    shared_secret TEXT NOT NULL,
    previous_shared_secret TEXT,
    secret_rotated_at TIMESTAMPTZ,
    secret_grace_hours INT NOT NULL DEFAULT 48,
    status TEXT NOT NULL DEFAULT 'enabled',
    consecutive_failures INT NOT NULL DEFAULT 0,
    last_error TEXT,
    last_delivery_at TIMESTAMPTZ,
    last_ack_sequence BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ssf_streams_tenant ON ssf_streams(tenant_id) WHERE status = 'enabled';

-- SSF Outbox
CREATE TABLE IF NOT EXISTS ssf_outbox (
    id BIGSERIAL PRIMARY KEY,
    tenant_id TEXT NOT NULL,
    stream_id UUID NOT NULL REFERENCES ssf_streams(id) ON DELETE CASCADE,
    sequence_number BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    set_token TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    attempts INT NOT NULL DEFAULT 0,
    next_attempt_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    delivered_at TIMESTAMPTZ,
    last_error TEXT
);

CREATE INDEX IF NOT EXISTS idx_ssf_outbox_pending
  ON ssf_outbox(next_attempt_at)
  WHERE status = 'pending';

CREATE INDEX IF NOT EXISTS idx_ssf_outbox_stream_seq
  ON ssf_outbox(stream_id, sequence_number);

-- Sequence counter per stream (for monotonic sequence numbers)
CREATE SEQUENCE IF NOT EXISTS ssf_sequence_seq;

-- Login geolocation for impossible travel
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS login_lat DOUBLE PRECISION;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS login_lon DOUBLE PRECISION;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS login_ip TEXT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS login_ip_type TEXT;
