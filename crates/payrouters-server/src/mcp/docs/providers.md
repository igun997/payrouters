# Provider accounts

One account per provider and mode, managed in the dashboard (**Providers**). Secrets are write-only there and always shown masked; they are deliberately not accepted through MCP.

Each account has its own **inbound webhook URL** (`{{base_url}}/hooks/<account-id>`, shown on the account card and by `list_provider_accounts`). Paste it into the provider dashboard:

| Provider | Credentials | Where to paste the webhook URL |
|---|---|---|
| Midtrans | Server key (sandbox or production) and, for notifications, the Signature Secret Key (Settings, Access Keys) | Settings, Configuration, Payment Notification URL |
| Xendit | Secret API key and the webhook verification token (`callback_token`) | Settings, Webhooks (payment and payment session events) |
| IDRX | API key and secret, default wallet address and chain id. Live only, no sandbox. | Dashboard, API Key, Callback URL (mint) |
| Mayar | API key (Read & Write; sandbox keys come from web.mayar.io) and optionally the Webhook Token (Integration, Webhook). Mayar does not document where the token travels, so when it is set any request header carrying it is accepted and a webhook without it is refused. | Integration, Webhook (one URL per Mayar account) |
| PayPal | Client ID and secret; optional fixed `idr_rate` (rupiah per US dollar; blank = ECB reference rate) and `webhook_id` | Developer, Apps & Credentials, the app's Webhooks: add the URL for Checkout order approved and Payment capture events, then copy its Webhook ID into the account |
| Mock | none, created automatically in sandbox | not needed: use the simulator |

Verification of inbound webhooks: Midtrans `signature_key` is SHA512(`order_id` + `status_code` + `gross_amount` + Signature Secret Key). Since 28 Oct 2026 that is a separate key from the server key (the "Test notification URL" button fails with a 401 when it is missing), so add it to the provider account in the dashboard; the server key is still accepted for merchants who never rotated; Xendit sends the account's callback token in `x-callback-token`; IDRX callbacks are unsigned, so PayRouters re-fetches every one from IDRX Transaction History before applying it; Mayar webhooks are unsigned too, so PayRouters re-reads the payment request from Mayar's API and uses only that status; PayPal webhooks are verified with PayPal's verify-webhook-signature call (needs `webhook_id`) and the order is then read again from PayPal before anything changes.

Mayar and PayPal have no standard methods (no virtual account numbers or QR codes through their APIs): every payment on them is the provider's hosted page, whatever `checkout` says, and `instructions` is a `redirect`.

PayPal cannot charge rupiah and PayRouters stays in IDR, so PayPal always charges in USD: the customer is charged `amount ÷ rate` US dollars, rounded up to the next cent, and the Snap page shows that amount and the rate before the customer picks PayPal and before the redirect. The rate is the European Central Bank's daily reference rate (public, cross of its IDR and USD rates, cached for an hour; a payment is refused when the latest ECB rate is older than 7 days) unless the account sets a fixed `idr_rate`. ECB rates are mid-market and published once per working day, so set a fixed `idr_rate` if you want a margin built in. The rate and its source are stored with the payment (`provider_data` in the dashboard), and the fee PayPal reports is converted back at the same rate. An approved PayPal order must be captured; PayRouters does that when the payer returns, on the approval webhook or when it polls. PayPal keeps an unapproved order open for about 3 hours, so a payer who comes back later than that has to be sent a new payment.

The optional per-method **fee schedule** (`fixed` rupiah and `percent`) is used when a provider does not report its fee, to fill `fee_amount` and `net_amount`.
