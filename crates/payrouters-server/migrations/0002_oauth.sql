-- OAuth 2.1 for the MCP server: dynamically registered public clients, single-use authorization
-- codes, grants (one per approved connection) and short-lived access tokens. Codes and tokens are
-- stored as SHA-256 hashes only.

CREATE TABLE oauth_clients (
    client_id     text        PRIMARY KEY,
    client_name   text        NOT NULL,
    redirect_uris jsonb       NOT NULL,
    created_at    timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE oauth_codes (
    code_hash      text        PRIMARY KEY,
    client_id      text        NOT NULL REFERENCES oauth_clients (client_id) ON DELETE CASCADE,
    redirect_uri   text        NOT NULL,
    code_challenge text        NOT NULL,
    scopes         text        NOT NULL,
    resource       text,
    expires_at     timestamptz NOT NULL
);

CREATE TABLE oauth_grants (
    id                 uuid        PRIMARY KEY,
    client_id          text        NOT NULL REFERENCES oauth_clients (client_id) ON DELETE CASCADE,
    scopes             text        NOT NULL,
    refresh_hash       text        NOT NULL,
    prev_refresh_hash  text,
    refresh_expires_at timestamptz NOT NULL,
    created_at         timestamptz NOT NULL DEFAULT now(),
    last_used_at       timestamptz,
    revoked_at         timestamptz
);
CREATE INDEX oauth_grants_refresh_idx ON oauth_grants (refresh_hash);
CREATE INDEX oauth_grants_prev_refresh_idx ON oauth_grants (prev_refresh_hash);
CREATE INDEX oauth_grants_client_idx ON oauth_grants (client_id);

CREATE TABLE oauth_access_tokens (
    token_hash text        PRIMARY KEY,
    grant_id   uuid        NOT NULL REFERENCES oauth_grants (id) ON DELETE CASCADE,
    expires_at timestamptz NOT NULL
);
CREATE INDEX oauth_access_tokens_expiry_idx ON oauth_access_tokens (expires_at);
