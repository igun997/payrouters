# Payment status

| Status | Meaning |
|---|---|
| `pending` | Created and waiting for the customer. The only non-terminal status. |
| `paid` | Settled. Final: it never changes afterwards. `fee_amount` and `net_amount` are set and one cash-in ledger row exists. |
| `expired` | Not paid before `expires_at`. A provider can still report a late payment: a payment marked `expired` may become `paid`, so handle `payment.paid` after `payment.expired`. |
| `failed` | The provider rejected or failed the charge. |
| `cancelled` | Cancelled before it was paid. |

PayRouters polls providers in the background (webhooks get lost), so a status usually settles on its own. Prefer the webhook over polling; use `sync` to force a re-read.
