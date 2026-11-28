-- Which payment options a product offers, and which provider serves each one, so one product can
-- combine providers (QRIS on Midtrans, virtual accounts on Xendit, PayPal for overseas customers).
-- `option` is a standard method code (`qris`, `va_bca`, ...) or `checkout:<provider>` for that
-- provider's hosted checkout page. A product without rows keeps the single-provider behaviour.

CREATE TABLE product_payment_options (
    product_id uuid    NOT NULL REFERENCES products (id) ON DELETE CASCADE,
    option     text    NOT NULL,
    provider   text    NOT NULL CHECK (provider IN ('midtrans', 'xendit', 'idrx', 'mayar', 'paypal', 'mock')),
    position   integer NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, option)
);

-- A payment created from those options picks its provider when the customer chooses an option, so
-- the Snap page may still move it to another provider until a charge exists.
ALTER TABLE payments ADD COLUMN routed boolean NOT NULL DEFAULT false;
