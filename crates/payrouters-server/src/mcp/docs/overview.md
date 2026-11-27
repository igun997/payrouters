# PayRouters overview

PayRouters is one API in front of Midtrans, Xendit and IDRX for **cash-in** payments in IDR. A product (your app) creates a payment, sends the customer to a hosted page, and learns the result from a signed webhook.

- Base URL of this deployment: `{{base_url}}`
- MCP endpoint (what you are connected to): `{{mcp_url}}`
- Amounts are integer rupiah (`150000`), `currency` is always `IDR`, timestamps are UTC (RFC 3339), JSON is `snake_case`.
- Two modes: `sandbox` (test keys `prk_test_...`, Midtrans sandbox, Xendit test keys and a built-in `mock` simulator) and `live` (`prk_live_...`). A key is bound to one mode and sandbox data never mixes into live data.

## The shortest integration

1. A **product** with a `webhook_url` (your backend endpoint that receives payment results) and a **sandbox API key** (`prk_test_...`). Both come from the tools `create_product` and `create_api_key`; the key and the `whsec_...` webhook secret are shown once.
2. Your backend calls `POST {{base_url}}/v1/payments` with `Authorization: Bearer prk_test_...` (see `create-payment`).
3. Send the customer to `snap_url` (the hosted page) or show `instructions` yourself.
4. Your webhook endpoint verifies `PayRouters-Signature` with the webhook secret and marks the order paid on `payment.paid` (see `webhooks`).
5. Test with the `mock` provider: `run_integration_selftest` checks the webhook endpoint in one call; for the checkout run the application, `wait_for_payment`, then `simulate_payment` (topic `testing`).

Environment variables for the product's backend:

```
PAYROUTERS_BASE_URL={{base_url}}
PAYROUTERS_API_KEY=prk_test_...        # from create_api_key (shown once)
PAYROUTERS_WEBHOOK_SECRET=whsec_...    # from create_product or rotate_webhook_secret (shown once)
```
