# Authentication

Your backend authenticates with a **product API key** as a bearer token:

```
Authorization: Bearer prk_test_...
```

- `prk_test_...` keys reach sandbox provider accounts only. `prk_live_...` keys reach live accounts only.
- A key belongs to one product; every payment it creates belongs to that product.
- The full key is shown once, when it is created (`create_api_key`). Only a prefix is stored for display. Lost keys are revoked and replaced, never recovered.
- Keep keys on the server. Never ship them in a browser or mobile app.
- Repeated failed attempts lock the caller out for a few minutes (`429 rate_limited` with `Retry-After`), even with a correct key.

The dashboard and this MCP connection use other credentials (the admin token and an OAuth token); they are not for your backend.
