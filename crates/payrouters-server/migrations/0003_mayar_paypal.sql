-- Mayar and PayPal join the provider list.

ALTER TABLE products DROP CONSTRAINT products_default_provider_check;
ALTER TABLE products ADD CONSTRAINT products_default_provider_check
    CHECK (default_provider IN ('midtrans', 'xendit', 'idrx', 'mayar', 'paypal', 'mock'));

ALTER TABLE provider_accounts DROP CONSTRAINT provider_accounts_provider_check;
ALTER TABLE provider_accounts ADD CONSTRAINT provider_accounts_provider_check
    CHECK (provider IN ('midtrans', 'xendit', 'idrx', 'mayar', 'paypal', 'mock'));
