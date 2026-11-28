# Create a payment

`POST {{base_url}}/v1/payments` with a product API key. Only `order_id` and `amount` are required.

```json
{
  "order_id": "INV-2026-0001",
  "amount": 150000,
  "provider": "midtrans",
  "checkout": "snap",
  "method": "va_bca",
  "description": "Order #1",
  "customer": { "name": "Budi", "email": "budi@example.com", "phone": "+628123456789" },
  "items": [ { "id": "sku-1", "name": "Pro plan", "price": 150000, "quantity": 1 } ],
  "metadata": { "user_id": "42" },
  "expires_in": 3600,
  "return_url": "https://app.example.com/thanks"
}
```

| Field | Notes |
|---|---|
| `order_id` | Required. At most 64 characters, unique per product and mode. |
| `amount` | Required. Integer rupiah, at least 1. For IDRX it is the IDRX to mint, at least 20000. PayPal charges the equivalent in its charge currency (USD by default) at the ECB reference rate (or the account's fixed `idr_rate`). |
| `provider` | `midtrans`, `xendit`, `idrx`, `mayar`, `paypal` or `mock`. Omitted: if the product has **payment options** (topic `payment-options`) they decide the provider per method; otherwise the product's default provider, else the first enabled provider of the key's mode. An explicit `provider` bypasses the options. |
| `checkout` | `snap` (default, PayRouters hosted page on the provider's core API) or `provider` (the provider's own hosted page). |
| `method` | `va_bca`, `va_bni`, `va_bri`, `va_mandiri`, `va_permata`, `qris`. With `snap` and a method the charge is created now and `instructions` is returned; without one the customer picks on the Snap page. |
| `description`, `customer`, `items`, `metadata` | Optional. `items` prices must add up to `amount`. `metadata` is returned unchanged in payments and webhooks. |
| `expires_in` | Seconds. Default 86400, minimum 300. |
| `return_url` | Where the Snap page sends the customer once the payment is no longer pending. |
| `provider_options` | IDRX only: `destination_wallet_address`, `chain_id` (default to the provider account's values). |

Response `201` is the payment: `id`, `order_id`, `status` (`pending`), `mode`, `livemode`, `amount`, `payable_amount`, `provider`, `method`, `checkout`, `snap_url`, `instructions`, `expires_at`, ... Always show `payable_amount ?? amount` as the total to pay (some providers add their fee on top).

`instructions` is `null` until a method exists, then one of:

```json
{ "type": "virtual_account", "bank": "bca", "account_number": "1234567890", "account_name": null, "biller_code": null, "bill_key": null }
{ "type": "qris", "qr_string": "000201...", "qr_image_url": null }
{ "type": "redirect", "url": "https://provider-hosted-page" }
```

(`biller_code` and `bill_key` are Mandiri bill payment; then `account_number` is `null`. `qr_string` is an empty string when the provider returns only the QR image: use `qr_image_url` then.)

## Idempotency

The same `order_id` with the same `amount` and provider while the payment is still `pending` returns `200` with the existing payment and the header `PayRouters-Idempotent-Replay: true`. Any other duplicate is `409 order_id_conflict`. After a timeout, repeat the call with the same `order_id` instead of inventing a new one.

## Reading payments

- `GET {{base_url}}/v1/payments/:id`
- `GET {{base_url}}/v1/payments?order_id=...&status=paid&limit=50&cursor=...` returns `{ "data": [...], "next_cursor": null | "..." }`
- `POST {{base_url}}/v1/payments/:id/sync` re-reads the provider and applies any status change (use it when a webhook was missed).
