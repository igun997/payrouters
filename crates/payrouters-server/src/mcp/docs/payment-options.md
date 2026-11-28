# Payment options: one product, several providers

A product can offer a **checklist of payment options**, each served by one provider chosen for it. Example for an international shop: QRIS through Midtrans, virtual accounts through Xendit, and PayPal for overseas customers.

An option is one of:

- a **standard method** code: `qris`, `va_bca`, `va_bni`, `va_bri`, `va_mandiri`, `va_permata`;
- `checkout:<provider>`: that provider's **hosted checkout page** (`checkout:paypal`, `checkout:mayar`, `checkout:midtrans`, ...). PayPal and Mayar have no standard methods, so `checkout:paypal` and `checkout:mayar` are their only options.

```json
[
  { "option": "qris",            "provider": "midtrans" },
  { "option": "va_bca",          "provider": "xendit"   },
  { "option": "va_mandiri",      "provider": "xendit"   },
  { "option": "checkout:paypal", "provider": "paypal"   }
]
```

Rules:

- A method is served by **exactly one provider** per product (a duplicate is `400`). Several hosted checkouts may be enabled together.
- Every option must be something its provider can do (`qris` on PayPal is refused, `checkout:mock` too).
- The order of the list is kept; the first option decides which provider a payment is created on until the customer chooses (see below).
- A product **without options** keeps the old behaviour: the `provider` of the request, then the product's `default_provider`, then the first enabled provider.

## Set them

Dashboard: Products, the product's **Payment options** checklist. MCP: `get_product_payment_options` (what is set and what could be chosen: every provider with an enabled account, its methods, whether it has a hosted checkout), then `set_product_payment_options` with the full list (an empty list clears it). API: `GET` / `PUT {{base_url}}/admin/products/:id/payment-options` with the admin token, body `{ "options": [ { "option": "qris", "provider": "midtrans" } ] }`. A product that already has live API keys needs the `live` permission to change.

## How a payment is served

Options are chosen per product, not per mode. A mode uses the options whose provider has an **enabled account in that mode**; sandbox falls back to the single-provider behaviour (the `mock` simulator) when none of them has one, so a product stays testable before its live accounts exist. In live mode a product whose options have no enabled account answers `422 provider_unavailable`.

| Request (no `provider`) | What happens |
|---|---|
| `method: "qris"` | Charged on the provider chosen for `qris` right away. A method the product does not offer: `422 method_not_enabled`. |
| `checkout: "snap"`, no method | The customer picks on the Snap page, which lists the product's methods and its hosted checkouts. Each pick is charged on its own provider. A product whose only option is one hosted checkout (just PayPal) skips the page and goes straight there. |
| `checkout: "provider"` | The first enabled hosted checkout of the product. None: `422 checkout_unsupported`. |
| `provider: "xendit"` | Bypasses the options: the old behaviour with that provider (all its methods). |

`GET {{base_url}}/v1/methods` with the product's key lists the product's own options (grouped by provider) instead of every provider. On the Snap page `POST /snap/api/:token/pay` takes exactly one of `{ "method": "qris" }` or `{ "provider": "paypal" }` (a hosted checkout); the payment moves to that provider when the charge is created, so `provider` in the payment can change from the one it was created on until then.

## Things to know

- Instructions differ by option: `virtual_account` or `qris` for methods, `redirect` for hosted checkouts. Send the customer to `snap_url` unless you render instructions yourself.
- **PayPal** cannot charge rupiah. Amounts stay integer IDR; the customer is charged `amount ÷ rate` in the account's currency (USD by default) at the ECB reference rate, or a fixed `idr_rate` on the account. The rate is stored with the payment.
- A provider that refuses a method as "not activated" (Midtrans `402`, also when only its Core API is not activated; Xendit `CHANNEL_NOT_ACTIVATED`) is detected: the method is hidden for 30 minutes and its payments go to that provider's hosted checkout instead (see `checkout-and-methods`).
- Fees: each provider account keeps its own fee schedule; a payment uses the account that took the charge. Stats and webhooks report the provider that served it.
