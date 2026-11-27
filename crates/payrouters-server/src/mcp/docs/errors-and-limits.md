# Errors and limits

Errors are JSON with an HTTP status:

```json
{ "error": { "code": "invalid_request", "message": "amount must be at least 1", "details": null } }
```

| HTTP | `code` | When |
|---|---|---|
| 400 | `invalid_request` | A field is missing or malformed. |
| 401 | `unauthorized` | The key is missing, wrong or revoked. |
| 404 | `not_found` | No such payment for this key. |
| 409 | `order_id_conflict` | The `order_id` exists with a different amount or provider, or is no longer pending. |
| 409 | `method_already_selected` | The payment already has instructions. |
| 422 | `provider_unavailable` | No enabled provider account for this mode. Add one in the dashboard (Providers) or use `mock` in sandbox. |
| 422 | `method_unsupported` | The provider does not offer that method. |
| 422 | `sandbox_unsupported` | A sandbox key asked for IDRX, which has no sandbox. |
| 429 | `rate_limited` | Too many failed attempts or requests. Wait `Retry-After` seconds. |
| 502 | `provider_error` | The provider answered with an error. Safe to retry with the same `order_id`. |
| 500 | `internal` | Unexpected server error. |

Branch on `code`, never on the message text. List endpoints take `limit` (default 50, at most 200) and `cursor`, and return `next_cursor`.
