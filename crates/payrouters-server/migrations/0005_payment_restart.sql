-- A customer can start an expired payment over from its Snap page.
--   window_secs     the lifetime the payment was created with, so a restart opens the same window.
--   restarts        how often it was started over (capped).
--   previous_refs   provider references of earlier attempts: a payment still made on one of them
--                   finds this payment when the provider reports it.
ALTER TABLE payments
    ADD COLUMN restarts integer NOT NULL DEFAULT 0,
    ADD COLUMN window_secs bigint,
    ADD COLUMN previous_refs text[] NOT NULL DEFAULT '{}';

CREATE INDEX payments_previous_refs_idx ON payments USING gin (previous_refs);
