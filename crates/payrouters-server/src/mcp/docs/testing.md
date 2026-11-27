# Testing an integration in sandbox

Everything here is sandbox only (the `mock` provider, sandbox keys). Nothing touches live data and no real money moves.

## One call: `run_integration_selftest`

Needs the product to have a `webhook_url` that PayRouters can reach (public https; for a local server use a tunnel such as cloudflared or ngrok and `update_product`). It checks the application's endpoint:

| Check | A correct endpoint |
|---|---|
| `payment.paid`, `payment.failed`, `payment.expired` delivered | answers 2xx to each signed event |
| refuses a wrong signature | answers non-2xx (401) |
| refuses a missing signature | answers non-2xx |
| refuses a stale timestamp | answers non-2xx to a correctly signed event dated an hour ago (tolerance 300 s) |
| acknowledges a repeated event | answers 2xx to the same event id again, and changes nothing |

The result is a checklist with `pass`, `warn`, `fail` or `skipped` and what to fix. A 2xx on the repeat does not prove the application ignored it: check in its code or database that the order was fulfilled once.

## Test the real checkout

1. Run the application with the product's sandbox key (`get_env_template` gives the variables) and trigger a purchase in it.
2. `wait_for_payment` (with `product_id`, optionally `order_id`) returns the payment the application created through the API. Use provider `mock` with a `method` so the next step can pay it.
3. `simulate_payment` with `outcome` `paid`, `failed` or `expired`. When the product has a webhook URL the result also says what the application's endpoint answered.
4. Confirm in the application that the order changed once, and that failed and expired orders were not fulfilled.

`create_test_payment` makes a payment without the application, `test_webhook_integration` is the single-event version of the self-test, `retry_webhook_delivery` replays a failed delivery after a fix.

## What the sandbox cannot show

Midtrans and Xendit sandbox payments are paid with those providers' own simulators, not with `simulate_payment`. The mock provider follows the same pipeline (ledger, status, webhook), so a passing mock test proves the application's side.
