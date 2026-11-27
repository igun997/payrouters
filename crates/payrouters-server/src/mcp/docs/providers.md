# Provider accounts

One account per provider and mode, managed in the dashboard (**Providers**). Secrets are write-only there and always shown masked; they are deliberately not accepted through MCP.

Each account has its own **inbound webhook URL** (`{{base_url}}/hooks/<account-id>`, shown on the account card and by `list_provider_accounts`). Paste it into the provider dashboard:

| Provider | Credentials | Where to paste the webhook URL |
|---|---|---|
| Midtrans | Server key (sandbox or production) and, for notifications, the Signature Secret Key (Settings, Access Keys) | Settings, Configuration, Payment Notification URL |
| Xendit | Secret API key and the webhook verification token (`callback_token`) | Settings, Webhooks (payment and payment session events) |
| IDRX | API key and secret, default wallet address and chain id. Live only, no sandbox. | Dashboard, API Key, Callback URL (mint) |
| Mock | none, created automatically in sandbox | not needed: use the simulator |

Verification of inbound webhooks: Midtrans `signature_key` is SHA512(`order_id` + `status_code` + `gross_amount` + Signature Secret Key). Since 28 Oct 2026 that is a separate key from the server key (the "Test notification URL" button fails with a 401 when it is missing), so add it to the provider account in the dashboard; the server key is still accepted for merchants who never rotated; Xendit sends the account's callback token in `x-callback-token`; IDRX callbacks are unsigned, so PayRouters re-fetches every one from IDRX Transaction History before applying it.

The optional per-method **fee schedule** (`fixed` rupiah and `percent`) is used when a provider does not report its fee, to fill `fee_amount` and `net_amount`.
