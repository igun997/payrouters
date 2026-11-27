# Webhooks (PayRouters to your product)

When a payment reaches a terminal status PayRouters sends `POST <product.webhook_url>` with a JSON body:

```json
{
  "id": "evt_0192...",
  "type": "payment.paid",
  "livemode": false,
  "created_at": "2026-11-26T10:02:09Z",
  "data": { "payment": { "id": "0192...", "order_id": "INV-2026-0001", "status": "paid", "amount": 150000 } }
}
```

`type` is one of `payment.paid`, `payment.expired`, `payment.failed`, `payment.cancelled`. `data.payment` is the full payment object.

## Headers and signature

- `PayRouters-Signature: t=<unix seconds>,v1=<hex>`
- `PayRouters-Event-Id: evt_...` (same as the body `id`)
- `Content-Type: application/json`, `User-Agent: PayRouters/<version>`

`v1 = hex(HMAC_SHA256(webhook_secret, "<t>." + <raw body>))`. Verify against the **raw request body exactly as received**, not a re-serialized JSON object, compare in constant time, and reject a timestamp more than 300 seconds from now. Several `v1=` values may appear (secret rotation): accept the request when any matches. The secret is the product's `whsec_...`, shown once by `create_product` and `rotate_webhook_secret`. `generate_code` with task `verify_webhook` returns ready code.

## Delivery rules

- Answer with any `2xx` within 10 seconds. Redirects are not followed.
- Anything else (or a timeout) is retried after 10 s, 1 m, 5 m, 30 m, 2 h, 6 h, 12 h and 24 h, then marked `failed`; a failed delivery can be retried with `retry_webhook_delivery` or from the dashboard.
- Delivery is at least once: ignore an event whose `PayRouters-Event-Id` you already handled.
- Check what happened with `list_webhook_deliveries` (status, attempts, last status code, last error); `run_integration_selftest` checks your endpoint against every scenario.
