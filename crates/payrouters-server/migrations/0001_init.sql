-- PayRouters initial schema. Cash-in only, IDR only.

CREATE TABLE products (
    id                 uuid PRIMARY KEY,
    name               text        NOT NULL,
    slug               text        NOT NULL UNIQUE,
    webhook_url        text,
    webhook_secret_enc text        NOT NULL,
    default_provider   text        CHECK (default_provider IN ('midtrans', 'xendit', 'idrx', 'mock')),
    archived_at        timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    id           uuid PRIMARY KEY,
    product_id   uuid        NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    mode         text        NOT NULL CHECK (mode IN ('sandbox', 'live')),
    name         text        NOT NULL,
    prefix       text        NOT NULL,
    key_hash     text        NOT NULL UNIQUE,
    created_at   timestamptz NOT NULL DEFAULT now(),
    last_used_at timestamptz,
    revoked_at   timestamptz
);
CREATE INDEX api_keys_product_idx ON api_keys (product_id);

CREATE TABLE provider_accounts (
    id              uuid PRIMARY KEY,
    provider        text        NOT NULL CHECK (provider IN ('midtrans', 'xendit', 'idrx', 'mock')),
    mode            text        NOT NULL CHECK (mode IN ('sandbox', 'live')),
    label           text        NOT NULL,
    enabled         boolean     NOT NULL DEFAULT true,
    credentials_enc text        NOT NULL,
    fees            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    UNIQUE (provider, mode)
);

CREATE TABLE payments (
    id                  uuid PRIMARY KEY,
    product_id          uuid        NOT NULL REFERENCES products (id),
    provider_account_id uuid        NOT NULL REFERENCES provider_accounts (id),
    order_id            text        NOT NULL,
    mode                text        NOT NULL CHECK (mode IN ('sandbox', 'live')),
    provider            text        NOT NULL,
    status              text        NOT NULL DEFAULT 'pending'
                                    CHECK (status IN ('pending', 'paid', 'expired', 'failed', 'cancelled')),
    amount              bigint      NOT NULL CHECK (amount > 0),
    currency            text        NOT NULL DEFAULT 'IDR',
    fee_amount          bigint,
    net_amount          bigint,
    payable_amount      bigint,
    method              text,
    checkout            text        NOT NULL CHECK (checkout IN ('snap', 'provider')),
    snap_token          text        NOT NULL UNIQUE,
    instructions        jsonb,
    description         text,
    customer            jsonb,
    items               jsonb       NOT NULL DEFAULT '[]'::jsonb,
    metadata            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    return_url          text,
    provider_options    jsonb       NOT NULL DEFAULT '{}'::jsonb,
    provider_ref        text,
    provider_data       jsonb       NOT NULL DEFAULT '{}'::jsonb,
    expires_at          timestamptz NOT NULL,
    paid_at             timestamptz,
    last_synced_at      timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    UNIQUE (product_id, mode, order_id)
);
CREATE INDEX payments_created_idx ON payments (created_at DESC, id DESC);
CREATE INDEX payments_provider_ref_idx ON payments (provider_account_id, provider_ref);
CREATE INDEX payments_pending_idx ON payments (expires_at) WHERE status = 'pending';

-- Raw inbox of everything providers send us (audit + replay).
CREATE TABLE provider_events (
    id                  uuid PRIMARY KEY,
    provider_account_id uuid        NOT NULL REFERENCES provider_accounts (id) ON DELETE CASCADE,
    provider            text        NOT NULL,
    payment_id          uuid        REFERENCES payments (id) ON DELETE SET NULL,
    verified            boolean     NOT NULL,
    outcome             text        NOT NULL CHECK (outcome IN ('applied', 'noop', 'unmatched', 'rejected', 'error')),
    error               text,
    headers             jsonb       NOT NULL DEFAULT '{}'::jsonb,
    body_text           text        NOT NULL,
    received_at         timestamptz NOT NULL DEFAULT now()
);
CREATE INDEX provider_events_received_idx ON provider_events (received_at DESC, id DESC);
CREATE INDEX provider_events_payment_idx ON provider_events (payment_id);

-- Cash-in ledger: exactly one row per paid payment.
CREATE TABLE cash_ins (
    id                  uuid PRIMARY KEY,
    payment_id          uuid        NOT NULL UNIQUE REFERENCES payments (id),
    product_id          uuid        NOT NULL REFERENCES products (id),
    provider_account_id uuid        NOT NULL REFERENCES provider_accounts (id),
    provider            text        NOT NULL,
    mode                text        NOT NULL CHECK (mode IN ('sandbox', 'live')),
    method              text,
    currency            text        NOT NULL DEFAULT 'IDR',
    gross_amount        bigint      NOT NULL,
    fee_amount          bigint      NOT NULL DEFAULT 0,
    net_amount          bigint      NOT NULL,
    paid_at             timestamptz NOT NULL,
    provider_ref        text,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CHECK (net_amount = gross_amount - fee_amount)
);
CREATE INDEX cash_ins_paid_idx ON cash_ins (mode, paid_at DESC, id DESC);
CREATE INDEX cash_ins_product_idx ON cash_ins (product_id, paid_at DESC);

-- Outbound webhook outbox (PayRouters -> products).
CREATE TABLE webhook_deliveries (
    id               uuid PRIMARY KEY,
    event_id         text        NOT NULL UNIQUE,
    payment_id       uuid        NOT NULL REFERENCES payments (id),
    product_id       uuid        NOT NULL REFERENCES products (id),
    event_type       text        NOT NULL,
    payload          jsonb       NOT NULL,
    status           text        NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'delivered', 'failed')),
    attempts         integer     NOT NULL DEFAULT 0,
    next_attempt_at  timestamptz,
    last_status_code integer,
    last_error       text,
    created_at       timestamptz NOT NULL DEFAULT now(),
    delivered_at     timestamptz
);
CREATE INDEX webhook_deliveries_due_idx ON webhook_deliveries (next_attempt_at) WHERE status = 'pending';
CREATE INDEX webhook_deliveries_created_idx ON webhook_deliveries (created_at DESC, id DESC);
CREATE INDEX webhook_deliveries_payment_idx ON webhook_deliveries (payment_id);
