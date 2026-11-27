# PayRouters API contract (v1)

Single binary (Rust/axum + Postgres). Three audiences, three auth schemes:

| Surface | Prefix | Auth | Caller |
|---|---|---|---|
| Product API | `/v1/*` | `Authorization: Bearer prk_test_…` or `prk_live_…` | your products (via SDK) |
| Admin API | `/admin/*` | `Authorization: Bearer <ADMIN_TOKEN>` | the dashboard (SPA) |
| Snap (public) | `/snap/api/:token` | none, token is the capability | customer browser |
| Provider webhooks (inbound) | `POST /hooks/:provider_account_id` | provider-specific verification | Midtrans / Xendit / IDRX |
| MCP server | `POST /mcp` | OAuth access token (`prm_at_…`) obtained through the dashboard session | AI assistants (Claude Code, Cursor, …); see `docs/mcp.md` |

The SPA is served by the same binary from `WEB_DIR` (SvelteKit `adapter-static`, SPA fallback → `index.html`). Same origin, no CORS. Any path not matching `/v1`, `/admin`, `/snap/api`, `/hooks`, `/mcp`, `/oauth/register`, `/oauth/token`, `/.well-known/*`, `/healthz` is the SPA (including `/snap/<token>`, the customer-facing page, and `/oauth/authorize`, the MCP consent page).

## Conventions

- JSON, `snake_case`. Amounts are **integer rupiah** (`amount: 150000`), `currency` is always `"IDR"`.
- Timestamps: RFC 3339 UTC strings. IDs: UUID strings.
- **Mode**: `"sandbox"` | `"live"`. A product API key is bound to one mode (`prk_test_` → sandbox, `prk_live_` → live). Every payment, provider account, cash-in carries `mode`. Sandbox data never mixes into live stats unless asked.
- Errors: HTTP status + body
  ```json
  { "error": { "code": "invalid_request", "message": "human text", "details": null } }
  ```
  Codes: `invalid_request` 400, `unauthorized` 401, `not_found` 404, `conflict` 409 (`order_id_conflict`, `method_already_selected`, …use `code` literally), `unprocessable` 422 (`provider_unavailable`, `method_unsupported`, `sandbox_unsupported`), `rate_limited` 429 (with `Retry-After` seconds; failed-auth lockout on `/admin`, `/v1`, `/mcp` and `/oauth/token`, per-IP request limits on `/snap/api`, `/hooks` and `/oauth/*`, tunable by `RATE_*` env), `provider_error` 502, `internal` 500.
- Pagination (lists): `?limit=` (default 50, max 200) & `?cursor=`; response `{ "data": [...], "next_cursor": "opaque"|null }`.
- Date filters: `from` / `to` are `YYYY-MM-DD` in the server's stats timezone (default `Asia/Jakarta`), inclusive.

## Enums

- `status`: `pending | paid | expired | failed | cancelled` (terminal: all but `pending`; `paid` is final).
- `provider`: `midtrans | xendit | idrx | mock`.
- `method` (standard codes): `va_bca | va_bni | va_bri | va_mandiri | va_permata | qris`. `null` while the customer has not chosen (snap) or when the provider-hosted page decides.
- `checkout`: `snap` (PayRouters hosted page, provider **core API**) | `provider` (provider-hosted page: Midtrans Snap, Xendit Payment Session, IDRX checkout).
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
- Response `201` = Payment. **Idempotent replay**: same `order_id` + same `amount` + same resolved provider on a still-`pending` payment → `200` with the existing payment and header `PayRouters-Idempotent-Replay: true`. Any other duplicate → `409 order_id_conflict`.
- Sandbox keys only reach `mode=sandbox` provider accounts (Midtrans sandbox, Xendit test keys, `mock`). IDRX has no sandbox → `422 sandbox_unsupported`.

### `GET /v1/payments/:id` → Payment
### `GET /v1/payments?order_id=&status=&limit=&cursor=` → `{data:[Payment], next_cursor}`
### `POST /v1/payments/:id/sync` → Payment (re-reads the provider, applies any status change)
### `GET /v1/methods` → catalog for this key's mode
```json
{ "data": [ { "provider": "midtrans", "checkout_provider": true, "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" } ] } ] }
```
(`group`: `virtual_account | qris`.) Only providers that have an enabled account in the key's mode.

---

## Snap (public, `/snap/api/:token`)

`token` = `payment.snap_url`'s last path segment.

### `GET /snap/api/:token`
```json
{
  "merchant_name": "My Product",
  "payment": { "id": "…", "order_id": "…", "status": "pending", "livemode": false, "amount": 150000, "payable_amount": null, "currency": "IDR", "provider": "midtrans", "method": null, "description": null, "items": [], "instructions": null, "expires_at": "…", "paid_at": null, "return_url": null },
  "methods": [ { "code": "va_bca", "label": "BCA Virtual Account", "group": "virtual_account" }, { "code": "qris", "label": "QRIS", "group": "qris" } ],
  "allow_simulate": false
}
```
`methods` = what the payment's provider supports in core mode (empty once `instructions` exist or status ≠ pending). `allow_simulate` is true only for the `mock` provider while pending. The page polls this endpoint (every 3 s) until `status` ≠ `pending`, then redirects to `return_url` if present.

### `POST /snap/api/:token/pay` body `{ "method": "va_bca" }` → same shape as GET
`409 method_already_selected` if instructions exist; `422 method_unsupported`; `409 conflict` if not pending/expired.

### `POST /snap/api/:token/simulate` body `{ "outcome": "paid" }` → same shape (mock provider only, else `422`)

---

## Provider webhooks (inbound)

`POST /hooks/:provider_account_id` — the single URL to paste into each provider dashboard (one per provider account). Always stores the raw request in the inbox. Responses: `200 {"ok":true}` when authentic (even if unmatched/duplicate/no-op), `401` when verification fails, `404` unknown account.
Verification: Midtrans `signature_key` = SHA512(`order_id`+`status_code`+`gross_amount`+Signature Secret Key); the account's optional `signature_secret_key` credential is tried first and the server key second (before 28 Oct 2026 the server key was the signing key and merchants who never rotated still have identical values); Xendit `x-callback-token` header = account `callback_token`; IDRX has no signature, so the status is re-fetched from IDRX Transaction History by `merchantOrderId` before anything is applied.

---

## Admin API (`/admin`, bearer `ADMIN_TOKEN`)

### Meta
- `GET /admin/me` → `{ "ok": true, "version": "0.1.0", "public_base_url": "https://…", "timezone": "Asia/Jakarta" }` (use to validate a token on login).
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
  "api_keys": [ { "id": "…", "mode": "sandbox", "name": "backend", "prefix": "prk_test_ab12", "created_at": "…", "last_used_at": "…|null", "revoked_at": "…|null" } ],
  "created_at": "…" }
```
- `GET /admin/products` → `{data:[Product]}` (archived hidden)
- `POST /admin/products` `{name, webhook_url?, default_provider?}` → `201 {…Product, "webhook_secret": "whsec_…"}` (secret shown only here and on rotate)
- `PATCH /admin/products/:id` `{name?, webhook_url?, default_provider?}` (`null` clears) → Product
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
