# PayRouters API contract (v1)

Single binary (Rust/axum + Postgres). Three audiences, three auth schemes:

| Surface | Prefix | Auth | Caller |
|---|---|---|---|
| Product API | `/v1/*` | `Authorization: Bearer prk_test_…` or `prk_live_…` | your products (via SDK) |
| Admin API | `/admin/*` | `Authorization: Bearer <ADMIN_TOKEN>` | the dashboard (SPA) |
| Snap (public) | `/snap/api/:token` | none, token is the capability | customer browser |
| Provider webhooks (inbound) | `POST /hooks/:provider_account_id` | provider-specific verification | Midtrans / Xendit / IDRX / Mayar / PayPal |
| MCP server | `POST /mcp` | OAuth access token (`prm_at_…`) obtained through the dashboard session | AI assistants (Claude Code, Cursor, …); see `docs/mcp.md` |

The SPA is served by the same binary from `WEB_DIR` (SvelteKit `adapter-static`, SPA fallback → `index.html`). Same origin, no CORS. Any path not matching `/v1`, `/admin`, `/snap/api`, `/hooks`, `/mcp`, `/oauth/register`, `/oauth/token`, `/.well-known/*`, `/assets/logo/*`, `/healthz` is the SPA (including `/snap/<token>`, the customer-facing page, and `/oauth/authorize`, the MCP consent page).

## Conventions

- JSON, `snake_case`. Amounts are **integer rupiah** (`amount: 150000`), `currency` is always `"IDR"`.
- Timestamps: RFC 3339 UTC strings. IDs: UUID strings.
- **Mode**: `"sandbox"` | `"live"`. A product API key is bound to one mode (`prk_test_` → sandbox, `prk_live_` → live). Every payment, provider account, cash-in carries `mode`. Sandbox data never mixes into live stats unless asked.
- Errors: HTTP status + body
  ```json
  { "error": { "code": "invalid_request", "message": "human text", "details": null } }
  ```
  Codes: `invalid_request` 400, `unauthorized` 401, `not_found` 404, `conflict` 409 (`order_id_conflict`, `method_already_selected`, …use `code` literally), `unprocessable` 422 (`provider_unavailable`, `method_unsupported`, `method_not_enabled`, `checkout_unsupported`, `channel_not_activated`, `sandbox_unsupported`), `rate_limited` 429 (with `Retry-After` seconds; failed-auth lockout on `/admin`, `/v1`, `/mcp` and `/oauth/token`, per-IP request limits on `/snap/api`, `/hooks` and `/oauth/*`, tunable by `RATE_*` env), `provider_error` 502, `internal` 500. A reverse proxy may replace a 5xx body with its own page (Easypanel does for 502): the server logs every failed provider charge with the provider's message (`provider charge failed`), so read the log when `provider_error` shows up as HTML.
- Pagination (lists): `?limit=` (default 50, max 200) & `?cursor=`; response `{ "data": [...], "next_cursor": "opaque"|null }`.
- Date filters: `from` / `to` are `YYYY-MM-DD` in the server's stats timezone (default `Asia/Jakarta`), inclusive.

## Enums

- `status`: `pending | paid | expired | failed | cancelled` (terminal: all but `pending`; `paid` is final).
- `provider`: `midtrans | xendit | idrx | mayar | paypal | mock`.
- `method` (standard codes): `va_bca | va_bni | va_bri | va_mandiri | va_permata | qris`. `null` while the customer has not chosen (snap) or when the provider-hosted page decides.
- `checkout`: `snap` (PayRouters hosted page, provider **core API**) | `provider` (provider-hosted page: Midtrans Snap, Xendit Payment Session, IDRX checkout, Mayar payment request, PayPal checkout). Mayar and PayPal have no standard methods: with `snap` and no `method` their hosted page is created straight away (`instructions.type = "redirect"`), and a `method` is refused with `422 method_unsupported`.
- `outcome` (simulate): `paid | expired | failed`.

## Objects

### Payment
```json
{
  "id": "0192…",
  "order_id": "INV-2026-0001",
  "status": "pending",
  "mode": "sandbox",
  "livemode": false,
  "amount": 150000,
  "currency": "IDR",
  "fee_amount": null,
  "net_amount": null,
  "payable_amount": null,
  "provider": "midtrans",
  "method": "va_bca",
  "checkout": "snap",
  "snap_url": "https://pay.example.com/snap/Zx9…",
  "instructions": { "type": "virtual_account", "bank": "bca", "account_number": "1234567890", "account_name": null, "biller_code": null, "bill_key": null },
  "description": "Order #1",
  "customer": { "name": "Budi", "email": "budi@example.com", "phone": "+628123456789" },
  "items": [ { "id": "sku-1", "name": "Pro plan", "price": 150000, "quantity": 1 } ],
  "metadata": { "user_id": "42" },
  "return_url": "https://app.example.com/thanks",
  "provider_ref": "…",
  "expires_at": "2026-11-27T10:00:00Z",
  "paid_at": null,
  "created_at": "2026-11-26T10:00:00Z",
  "updated_at": "2026-11-26T10:00:00Z"
}
```
`fee_amount`/`net_amount` are `null` until paid. `payable_amount` is the exact total the customer must transfer when the provider adds its fee on top (IDRX VA); `null` means pay `amount` — always display `payable_amount ?? amount` as "Total to pay". `instructions` is `null` until a method/checkout exists. `customer`, `items`, `description`, `return_url` may be `null`/`[]`.

### Instructions (tagged by `type`)
```json
{ "type": "virtual_account", "bank": "bca", "account_number": "…", "account_name": "…|null", "biller_code": "…|null", "bill_key": "…|null" }
{ "type": "qris", "qr_string": "000201…", "qr_image_url": "https://…|null" }
{ "type": "redirect", "url": "https://provider-hosted-page" }
```
(`biller_code`/`bill_key` are Mandiri bill payment; then `account_number` is `null`. `qr_string` is an empty string when a provider returns only the QR image: use `qr_image_url` then.)

### Webhook event (PayRouters → your product)
`POST <product.webhook_url>`
```json
{ "id": "evt_0192…", "type": "payment.paid", "livemode": false, "created_at": "…", "data": { "payment": { /* Payment */ } } }
```
Types: `payment.paid | payment.expired | payment.failed | payment.cancelled`.
Headers: `PayRouters-Signature: t=<unix_seconds>,v1=<hex>`, `PayRouters-Event-Id: evt_…`, `Content-Type: application/json`, `User-Agent: PayRouters/<ver>`.
`v1 = hex(HMAC_SHA256(webhook_secret, "<t>.<raw body>"))`. Reject if `|now - t| > 300s`. Respond `2xx` to ack. Non-2xx / timeout (10 s) → retried at 10s, 1m, 5m, 30m, 2h, 6h, 12h, 24h, then `failed` (manually retryable from the dashboard). Delivery is at-least-once: dedupe on `PayRouters-Event-Id`.

---

## Product API (`/v1`, bearer product key)

### `POST /v1/payments`
```json
{
  "order_id": "INV-2026-0001",
  "amount": 150000,
  "provider": "midtrans",
  "checkout": "snap",
  "method": "va_bca",
  "description": "Order #1",
  "customer": { "name": "Budi", "email": "b@x.id", "phone": "+628123456789" },
  "items": [ { "id": "sku-1", "name": "Pro plan", "price": 150000, "quantity": 1 } ],
  "metadata": { "user_id": "42" },
  "expires_in": 3600,
  "return_url": "https://app.example.com/thanks",
  "provider_options": { "destination_wallet_address": "0x…", "chain_id": 8453 }
}
```
- Required: `order_id` (≤64 chars, unique per product+mode), `amount` (≥ 1). Everything else optional.
- `provider` omitted → product's `default_provider`, else the first enabled provider of that mode.
- `checkout` default `snap`. With `snap` and no `method`, the customer picks on the snap page. With `snap` and `method`, the charge is created immediately and `instructions` is returned. With `provider`, the provider-hosted URL is returned as `instructions.type="redirect"`.
- `expires_in` seconds, default 86400 (24 h), min 300.
- `items` prices must sum to `amount` if given (else 400).
- `provider_options` is provider-specific. IDRX: `destination_wallet_address`, `chain_id` (fall back to the provider account defaults); IDRX `amount` is the IDRX to be minted (min 20000).
- PayPal cannot charge rupiah: `amount` stays integer IDR and the customer is charged `amount ÷ rate` in the account's charge currency (USD by default), rounded up to the next minor unit. The rate is the ECB's daily euro reference rate (cross of IDR and the currency; refused when older than 7 days) unless the provider account sets a fixed `idr_rate`. The charged currency and value, the rate and its source are in the payment's stored provider data. `currency` on the payment stays `IDR`, and `fee_amount` is the PayPal fee converted back at the same rate.
- Response `201` = Payment. **Idempotent replay**: same `order_id` + same `amount` + same resolved provider on a still-`pending` payment → `200` with the existing payment and header `PayRouters-Idempotent-Replay: true`. Any other duplicate → `409 order_id_conflict`.
- Sandbox keys only reach `mode=sandbox` provider accounts (Midtrans sandbox, Xendit test keys, Mayar sandbox, PayPal sandbox, `mock`). IDRX has no sandbox → `422 sandbox_unsupported`.
- **Channels the account does not have are detected, not configured.** When a provider answers a standard-method charge as "not activated" (Midtrans `402 Payment channel is not activated`, which also happens when only its Core API is not activated; Xendit `CHANNEL_NOT_ACTIVATED`), PayRouters remembers that method as inactive for the provider account for 30 minutes (in memory; editing the account clears it), stops offering it on the Snap page while another method is left, and serves the payment from the provider's hosted checkout instead: the payment comes back with `instructions.type = "redirect"` and `method: null`, even when `method` was sent. When every standard method is refused, a Snap payment goes to the hosted checkout straight away. A provider with no hosted checkout answers `422 channel_not_activated`.

### `GET /v1/payments/:id` → Payment
### `GET /v1/payments?order_id=&status=&limit=&cursor=` → `{data:[Payment], next_cursor}`
### `POST /v1/payments/:id/sync` → Payment (re-reads the provider, applies any status change)
### `GET /v1/methods` → catalog for this key's mode
```json
{ "data": [ { "provider": "midtrans", "checkout_provider": true, "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" } ] } ] }
```
(`group`: `virtual_account | qris`.) Only providers that have an enabled account in the key's mode. For a product with **payment options** (see below) the catalog is the product's own options grouped by provider: `methods` are the standard methods that provider serves for the product and `checkout_provider` tells whether the product offers that provider's hosted checkout (PayPal and Mayar: `methods` is empty, `checkout_provider` true).

---

## Snap (public, `/snap/api/:token`)

`token` = `payment.snap_url`'s last path segment.

### `GET /snap/api/:token`
```json
{
  "merchant_name": "My Product",
  "payment": { "id": "…", "order_id": "…", "status": "pending", "livemode": false, "amount": 150000, "payable_amount": null, "currency": "IDR", "provider": "midtrans", "method": null, "description": null, "items": [], "instructions": null, "expires_at": "…", "paid_at": null, "return_url": null },
  "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" }, { "code": "qris", "label": "QRIS", "group": "qris" } ],
  "checkouts": [ { "provider": "paypal", "label": "PayPal" } ],
  "allow_simulate": false
}
```
`methods` = what the customer can pick: the payment's provider in core mode, or, for a payment created from the product's payment options, the product's methods; `checkouts` = the product's hosted checkouts the customer can pick (always empty without payment options). Both are empty once `instructions` exist or status ≠ pending. A method a provider account has been seen to refuse as not activated is left out while anything else remains. `allow_simulate` is true only for the `mock` provider while pending. The page polls this endpoint (every 3 s) until `status` ≠ `pending`, then redirects to `return_url` if present.

### `POST /snap/api/:token/pay` body `{ "method": "va_bca" }` or `{ "provider": "paypal" }` → same shape as GET
Exactly one of `method` (a standard method) or `provider` (that provider's hosted checkout; the answer has `instructions.type = "redirect"`), else `400`. `409 method_already_selected` if instructions exist or a choice is in flight; `422 method_unsupported` / `422 checkout_unsupported` (the payment, or its product's options, do not offer it); `409 conflict` if not pending/expired. For a payment created from payment options the payment moves to the provider that serves the choice when the charge is created. A method the provider reports as not activated is served from its hosted checkout instead (`method` stays `null`).

### `GET /snap/api/:token/return`, `GET /snap/api/:token/cancel`
Where PayPal sends the payer back (set by the adapter as `return_url` / `cancel_url`, not meant to be called by products). `return` settles a pending payment with the provider (PayPal: captures an approved order) and answers `303` to the Snap page; `cancel` answers `303` to the Snap page with `?paused=1`, which stops the page's automatic hop to the provider. Unknown token: `404`.

### `POST /snap/api/:token/simulate` body `{ "outcome": "paid" }` → same shape (mock provider only, else `422`)

---

## Payment options (one product, several providers)

A product can offer a checklist of payment options, each served by one provider chosen for it: for example QRIS on Midtrans, virtual accounts on Xendit and PayPal for overseas customers. An option is a standard method code (`qris`, `va_bca`, `va_bni`, `va_bri`, `va_mandiri`, `va_permata`) or `checkout:<provider>` for that provider's hosted checkout page (`checkout:paypal`, `checkout:mayar`, `checkout:midtrans`, ...). PayPal and Mayar have no standard methods, so their only option is the checkout.

- A method is served by **one** provider per product (a duplicate is `400`); several hosted checkouts may be enabled. Each option must be something its provider supports. The list order is kept.
- A product **without** options keeps the single-provider behaviour: the request's `provider`, then the product's `default_provider`, then the first enabled provider.
- Options are per product, not per mode. A mode uses the options whose provider has an **enabled account in that mode**; sandbox falls back to the single-provider behaviour (the mock simulator) when none does; live answers `422 provider_unavailable`.

How `POST /v1/payments` is served when the request has no `provider`:

| Request | Result |
|---|---|
| `method: "qris"` | Charged at once on the provider chosen for `qris`. Not among the options: `422 method_not_enabled`. |
| `checkout: "snap"`, no `method` | The customer picks on the Snap page (`methods` plus `checkouts`); each pick is charged on its own provider. A product whose only option is one hosted checkout goes straight to it. The payment is created on the first option's provider and moves to the provider that takes the charge, so `provider` can change until then (the payment keeps `checkout: "snap"`). |
| `checkout: "provider"` | The product's first enabled hosted checkout; none: `422 checkout_unsupported`. |
| `provider: "xendit"` | Bypasses the options: the single-provider behaviour for that provider. |

Idempotent replay of a payment created from options does not compare the provider.

Admin API (bearer `ADMIN_TOKEN`):
- `GET /admin/products/:id/payment-options` → `{ "data": [ { "option": "qris", "provider": "midtrans" } ], "available": [ { "provider": "paypal", "label": "PayPal", "modes": ["sandbox"], "methods": [], "checkout": true } ] }`. `data` is the selection in order (`[]`: not configured); `available` lists every provider with an enabled account, the modes it has one in, the standard methods it supports and whether it has a hosted checkout.
- `PUT /admin/products/:id/payment-options` `{ "options": [ { "option": "qris", "provider": "midtrans" }, { "option": "checkout:paypal", "provider": "paypal" } ] }` replaces the selection (`[]` clears it) and answers like `GET`. `400` for an unknown option, an option its provider cannot serve, a repeated option or more than 40; `404` unknown product.
- Products carry the selection as `payment_options`. The MCP tools are `get_product_payment_options` and `set_product_payment_options` (topic `payment-options`).

Channels the provider has not activated are handled on top of this: see the note under `POST /v1/payments`.

---

## Provider webhooks (inbound)

`POST /hooks/:provider_account_id` — the single URL to paste into each provider dashboard (one per provider account). Always stores the raw request in the inbox. Responses: `200 {"ok":true}` when authentic (even if unmatched/duplicate/no-op), `401` when verification fails, `404` unknown account.
Verification: Midtrans `signature_key` = SHA512(`order_id`+`status_code`+`gross_amount`+Signature Secret Key); the account's optional `signature_secret_key` credential is tried first and the server key second (before 28 Oct 2026 the server key was the signing key and merchants who never rotated still have identical values); Xendit `x-callback-token` header = account `callback_token`; a Midtrans body with none of `order_id`, `transaction_status`, `gross_amount` (the recurring and account-linking notifications, and their dashboard test buttons; the account-linking one carries a `signature_key` Midtrans does not document how to verify) is acknowledged with 200 and ignored; IDRX has no signature, so the status is re-fetched from IDRX Transaction History by `merchantOrderId` before anything is applied.

Mayar documents no signature: a `payment.received` webhook only names the payment request (`data.productId`), and the status and amount come from `GET /hl/v2/payments/{id}` with the account's API key; other events and other products are acknowledged and ignored. When the account stores Mayar's dashboard Webhook Token (`webhook_token`), a request that no header carries it in (whole value or bearer) is refused with `401` listing the header names received; Mayar does not document the header, so none is assumed, and the token is never stored in the inbox. PayPal: the five `paypal-*` transmission headers and the raw body are posted to `/v1/notifications/verify-webhook-signature` with the account's `webhook_id` (`401` when it fails or `webhook_id` is unset); the event only names the order, which is read again (and captured when the payer has approved it) before anything changes. PayPal also returns the payer to `GET /snap/api/:token/return` (or `/cancel`), which settles the payment and redirects to the Snap page.

---

## Admin API (`/admin`, bearer `ADMIN_TOKEN`)

### Meta
- `GET /admin/me` → `{ "ok": true, "version": "0.5.0", "public_base_url": "https://…", "timezone": "Asia/Jakarta" }` (use to validate a token on login).
- `GET /admin/meta` →
```json
{
  "providers": [
    { "id": "midtrans", "label": "Midtrans", "supports_sandbox": true, "supports_provider_checkout": true,
      "credential_fields": [ { "key": "server_key", "label": "Server key", "secret": true, "required": true, "placeholder": "SB-Mid-server-…" } ],
      "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" } ] }
  ],
  "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" } ],
  "statuses": ["pending","paid","expired","failed","cancelled"]
}
```

### Products
Product:
```json
{ "id": "…", "name": "My App", "slug": "my-app", "webhook_url": "https://…|null", "default_provider": "midtrans|null",
  "payment_options": [ { "option": "qris", "provider": "midtrans" } ],
  "api_keys": [ { "id": "…", "mode": "sandbox", "name": "backend", "prefix": "prk_test_ab12", "created_at": "…", "last_used_at": "…|null", "revoked_at": "…|null" } ],
  "created_at": "…" }
```
- `GET /admin/products` → `{data:[Product]}` (archived hidden)
- `POST /admin/products` `{name, webhook_url?, default_provider?}` → `201 {…Product, "webhook_secret": "whsec_…"}` (secret shown only here and on rotate)
- `PATCH /admin/products/:id` `{name?, webhook_url?, default_provider?}` (`null` clears) → Product
- `GET|PUT /admin/products/:id/payment-options` → see **Payment options**
- `DELETE /admin/products/:id` → `204` (archives, revokes keys)
- `POST /admin/products/:id/webhook-secret/rotate` → `{ "webhook_secret": "whsec_…" }`
- `POST /admin/products/:id/api-keys` `{mode, name}` → `201 { …key meta, "key": "prk_test_<full secret>" }` (full key shown once)
- `DELETE /admin/products/:id/api-keys/:key_id` → `204` (revoke)

### Provider accounts
One account per (`provider`, `mode`).
ProviderAccount:
```json
{ "id": "…", "provider": "midtrans", "mode": "sandbox", "label": "Midtrans sandbox", "enabled": true,
  "credentials": { "server_key": "••••abcd" },
  "fees": { "va_bca": { "fixed": 4000, "percent": 0 } },
  "webhook_url": "https://pay.example.com/hooks/<id>",
  "created_at": "…" }
```
`credentials`: secret fields masked (`••••` + last 4, or `""` if unset), non-secret fields plain. `fees`: optional per-method fee schedule used when the provider does not report a fee (`fixed` rupiah + `percent` e.g. `0.7`); key `"*"` = default.

Credential keys per provider (`GET /admin/meta` lists them with labels, required flags and which are secret):

| Provider | Required | Optional |
|---|---|---|
| `midtrans` | `server_key` | `signature_secret_key` (webhook signatures) |
| `xendit` | `secret_key`, `callback_token` | |
| `idrx` | `api_key`, `api_secret` | `default_wallet_address`, `default_chain_id` |
| `mayar` | `api_key` | `webhook_token` |
| `paypal` | `client_id`, `client_secret` | `idr_rate` (fixed rupiah per unit; blank = ECB reference rate), `currency` (default `USD`), `webhook_id` |
| `mock` | none | |

- `GET /admin/providers` → `{data:[ProviderAccount]}`
- `POST /admin/providers` `{provider, mode, label?, credentials:{…}, fees?}` → `201`. Duplicate (provider,mode) → `409`; IDRX sandbox → `422 sandbox_unsupported`; missing required credential → `400`.
- `PATCH /admin/providers/:id` `{label?, enabled?, credentials?, fees?}` — credentials are merged per key; a value equal to its masked form or omitted keeps the stored one. → ProviderAccount
- `DELETE /admin/providers/:id` → `204`; `409` if payments reference it (disable instead).
- `POST /admin/providers/:id/test` → `{ "ok": true, "message": "Authenticated against Midtrans sandbox" }`

### Payments
AdminPayment = Payment + `{ "product": {"id","name"}, "provider_account_id": "…" }`.
- `GET /admin/payments?mode=&status=&provider=&product_id=&q=&from=&to=&limit=&cursor=` → `{data:[AdminPayment], next_cursor}` (`q` matches `order_id`, id, `provider_ref`; default `mode` = all)
- `GET /admin/payments/:id` →
```json
{ "payment": AdminPayment,
  "events": [ { "id": "…", "received_at": "…", "outcome": "applied", "verified": true, "error": null } ],
  "deliveries": [ Delivery ],
  "cash_in": CashIn | null }
```
- `POST /admin/payments` `{product_id, mode, …same body as /v1/payments}` → `201 AdminPayment` (dashboard playground; no API key needed)
- `POST /admin/payments/:id/sync` → AdminPayment
- `POST /admin/payments/:id/simulate` `{outcome}` → AdminPayment (mock only)

### Webhook inbox & deliveries
WebhookEvent: `{ "id", "provider", "provider_account_id", "payment_id": "…|null", "verified": true, "outcome": "applied|noop|unmatched|rejected|error", "error": null, "headers": {…}, "body": {…}|"raw string", "received_at" }` (auth-ish headers redacted).
- `GET /admin/webhook-events?provider_account_id=&outcome=&limit=&cursor=` → `{data:[WebhookEvent], next_cursor}`
Delivery: `{ "id", "payment_id", "product_id", "event_id", "event_type", "status": "pending|delivered|failed", "attempts", "next_attempt_at": "…|null", "last_status_code": 200|null, "last_error": null, "payload": {…}, "created_at", "delivered_at": "…|null" }`
- `GET /admin/deliveries?status=&product_id=&payment_id=&limit=&cursor=` → `{data:[Delivery], next_cursor}`
- `POST /admin/deliveries/:id/retry` → Delivery (status back to `pending`, due now)

### Stats & cash-in recap
Common query: `mode` (default `live`), `from`, `to` (default last 30 days), `product_id?`, `provider?`.
- `GET /admin/stats/summary` →
```json
{ "range": { "from": "2026-10-28", "to": "2026-11-26", "mode": "live" },
  "payments": { "total": 120, "pending": 3, "paid": 100, "expired": 12, "failed": 3, "cancelled": 2 },
  "conversion_rate": 0.8333,
  "cash_in": { "count": 100, "gross": 15000000, "fee": 120000, "net": 14880000, "average": 150000 },
  "previous": { "count": 80, "gross": 11000000, "fee": 90000, "net": 10910000 } }
```
(`previous` = the immediately preceding range of equal length; `conversion_rate` = paid / (paid+expired+failed+cancelled), `0` when none.)
- `GET /admin/stats/timeseries` → `{ "data": [ { "date": "2026-11-01", "created": 5, "count": 4, "gross": 600000, "fee": 4800, "net": 595200 } ] }` zero-filled, one row per day.
- `GET /admin/stats/breakdown?by=provider|method|product` → `{ "data": [ { "key": "midtrans", "label": "Midtrans", "created": 60, "paid": 50, "success_rate": 0.83, "gross": 7500000, "fee": 60000, "net": 7440000 } ] }` sorted by `gross` desc. For `by=method`, `key` may be `"other"`.
- `GET /admin/cash-ins?…&method=&limit=&cursor=` → `{ "data": [CashIn], "next_cursor": null, "totals": { "count": 100, "gross": 15000000, "fee": 120000, "net": 14880000 } }` (totals cover the whole filter, not only the page)
- `GET /admin/cash-ins/export.csv?…same filters` → `text/csv` (all rows, header: `paid_at,order_id,payment_id,product,provider,method,mode,gross,fee,net,provider_ref`)
CashIn: `{ "id", "payment_id", "order_id", "product": {"id","name"}, "provider", "method": "va_bca|null", "mode", "currency": "IDR", "gross_amount", "fee_amount", "net_amount", "paid_at", "provider_ref" }`.

## MCP server and OAuth

`POST /mcp` speaks the Model Context Protocol; its OAuth 2.1 flow (discovery, dynamic client registration, PKCE, consent through the signed-in dashboard session) and the tool list are in `docs/mcp.md`. The admin side of the consent screen and the connection list are `GET|POST /admin/oauth/authorize`, `GET /admin/oauth/grants` and `DELETE /admin/oauth/grants/:id`. `GET /admin/me` also returns `mcp_url`.

## Misc
- `GET /healthz` → `200 ok` (DB reachable).
- `GET /assets/logo/:code` (public, `snap` rate limit) → the bank or QRIS logo for `bca`, `bni`, `bri`, `mandiri`, `permata` or `qris`, as `image/png` or `image/jpeg`; any other code is `404`. The server fetches each logo once from the Google favicon service (`t1.gstatic.com/faviconV2`), keeps it in memory for 7 days and answers `404` for 10 minutes when the fetch fails, so the payer's browser never contacts a third party; the UI then keeps its text badge. The server needs outbound HTTPS for this; without it everything else works.
