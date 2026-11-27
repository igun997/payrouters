# PayRouters MCP server (OAuth)

PayRouters speaks the [Model Context Protocol](https://modelcontextprotocol.io) at `POST {PUBLIC_BASE_URL}/mcp`. An MCP client (Claude Code, Claude Desktop, Cursor, VS Code, Codex, ...) connects with OAuth 2.1: the client sends the user to the dashboard in their browser, the already signed-in dashboard session shows one consent screen, and the client receives its own token. Nobody pastes the admin token or an API key, and the assistant can read the live configuration instead of guessing field names from the documentation.

## What the user does

1. Dashboard → **Integrations** shows the server URL (`https://pay.example.com/mcp`) and a copy-ready command for each client, for example `claude mcp add --transport http payrouters https://pay.example.com/mcp`.
2. The client starts the OAuth flow and opens the browser at `/oauth/authorize`. With an active dashboard session the user sees what the client asks for and presses **Allow**. Without a session the dashboard asks for the admin token first and then returns to the consent screen.
3. The assistant can now ask PayRouters what is configured, create a product and a sandbox key, generate integration code with the right base URL, send a test payment and check that the webhook endpoint verifies the signature.
4. **Integrations → Connected apps** lists every connection with its last use and revokes it.

## Scopes

| Scope | Lets the client | Default on consent |
|---|---|---|
| `read` | Read products, provider accounts (secrets masked), payments, webhook events and deliveries, stats, docs | always on |
| `write` | Create and edit products, rotate webhook secrets, create and revoke API keys, create test payments, simulate, sync, retry deliveries | on |
| `live` | Extend `read` and `write` to **live mode** (live API keys, live payments, live data) | **off** |

Without `live` every tool works on sandbox data only: lists are filtered to sandbox, live keys are hidden and a `mode: "live"` argument is refused with an explanation. The consent screen lets the user untick `write` and tick `live`.

Never offered through MCP, by design: provider credentials (secrets must not pass through a chat), archiving products, deleting provider accounts, changing fee schedules.

## OAuth endpoints

All JSON, CORS open (`*`; bearer tokens are not ambient credentials), `Cache-Control: no-store` on token responses.

| Endpoint | Purpose |
|---|---|
| `GET /.well-known/oauth-protected-resource` and `/.well-known/oauth-protected-resource/mcp` | RFC 9728 metadata: `resource`, `authorization_servers`, `scopes_supported`, `bearer_methods_supported` |
| `GET /.well-known/oauth-authorization-server` (alias `/.well-known/openid-configuration`) | RFC 8414 metadata: `issuer`, `authorization_endpoint` (`/oauth/authorize`, a dashboard page), `token_endpoint`, `registration_endpoint`, `code_challenge_methods_supported: ["S256"]`, `grant_types_supported: ["authorization_code","refresh_token"]`, `token_endpoint_auth_methods_supported: ["none"]`, `scopes_supported` |
| `POST /oauth/register` | RFC 7591 dynamic client registration for public clients |
| `GET /oauth/authorize` | Dashboard consent page (SPA route) |
| `POST /oauth/token` | `application/x-www-form-urlencoded`: `authorization_code` (+ PKCE `code_verifier`) and `refresh_token` |
| `POST /mcp` | MCP over Streamable HTTP, JSON responses |

### Registration

`POST /oauth/register` with `client_name` (1-80 chars), `redirect_uris` (1-5) and optionally `token_endpoint_auth_method: "none"`. Redirect URIs must be `https`, `http` on `localhost`/`127.0.0.1`/`[::1]` (the port may differ at authorization time, RFC 8252), or a custom scheme (`cursor://...`); `javascript:`, `data:`, `file:` and fragments are refused. Answer `201`: `client_id`, `client_id_issued_at`, `client_name`, `redirect_uris`, `grant_types`, `response_types`, `token_endpoint_auth_method: "none"`. Errors: `400 {"error":"invalid_redirect_uri"|"invalid_client_metadata","error_description":"..."}`. Clients that never completed an authorization are pruned after 7 days.

### Authorization and consent

The client opens `/oauth/authorize?response_type=code&client_id=...&redirect_uri=...&code_challenge=...&code_challenge_method=S256&state=...&scope=read write&resource=https://pay.example.com/mcp`. PKCE with `S256` is mandatory. `resource` must equal the MCP URL when present. An empty `scope` means `read write`.

The page calls the admin API with the dashboard session (admin bearer token):

- `GET /admin/oauth/authorize?<same query>` validates the request and returns what to show:
  ```json
  {
    "client": { "client_id": "...", "client_name": "Claude Code" },
    "redirect_host": "127.0.0.1",
    "resource": "https://pay.example.com/mcp",
    "scopes": [
      { "key": "read", "label": "Read", "description": "...", "granted": true, "locked": true },
      { "key": "write", "label": "Make changes", "description": "...", "granted": true, "locked": false },
      { "key": "live", "label": "Live mode", "description": "...", "granted": false, "locked": false }
    ]
  }
  ```
  `granted` is the pre-checked state (what the client asked for, `read` always). An unknown client, an unregistered `redirect_uri`, a missing challenge and similar problems answer `400 invalid_request` with a message; the page shows it and never redirects to an unvalidated address.
- `POST /admin/oauth/authorize` with the same fields plus `scope` (the final space-separated scopes, must include `read` and be a subset of `read write live`) and `decision` (`"allow"` or `"deny"`) returns `{ "redirect_url": "<redirect_uri>?code=...&state=...&iss=..." }`; `deny` returns `...?error=access_denied&state=...&iss=...`. The page then navigates to `redirect_url`.

Codes are single use and expire after 10 minutes.

### Token

`POST /oauth/token`:

- `grant_type=authorization_code&code=...&redirect_uri=...&client_id=...&code_verifier=...` (+ optional `resource`)
- `grant_type=refresh_token&refresh_token=...&client_id=...` (+ optional narrower `scope`)

Answer `200`: `{ "access_token": "prm_at_...", "token_type": "Bearer", "expires_in": 3600, "refresh_token": "prm_rt_...", "scope": "read write" }`. Access tokens live 1 hour. Refresh tokens live 30 days, rotate on every use and a replayed (already rotated) refresh token revokes the whole connection. Errors follow RFC 6749: `400 invalid_request | invalid_grant | unsupported_grant_type | invalid_scope`, `401 invalid_client`.

### Connections (admin API)

- `GET /admin/oauth/grants` → `{ "data": [ { "id", "client_id", "client_name", "scopes": ["read","write"], "created_at", "last_used_at": null, "expires_at" } ] }`
- `DELETE /admin/oauth/grants/:id` → `204`. The access and refresh tokens stop working at once.
- `GET /admin/me` also returns `mcp_url`.

## MCP endpoint

`POST /mcp` with `Authorization: Bearer prm_at_...`. A missing or invalid token answers `401` with `WWW-Authenticate: Bearer resource_metadata="https://pay.example.com/.well-known/oauth-protected-resource/mcp", scope="read write"`, which is what makes a client start the OAuth flow by itself. `GET` and `DELETE` answer `405` (no server-initiated stream, no session id).

Implemented methods: `initialize`, `notifications/initialized` (202), `ping`, `tools/list`, `tools/call`, `resources/list`, `resources/read`, `prompts/list`, `prompts/get`. Batches (JSON arrays) are accepted. Supported protocol versions: `2025-11-25`, `2025-06-18`, `2025-03-26`, `2024-11-05`.

A tool result is `{ "content": [{ "type": "text", "text": "..." }], "structuredContent": {...}, "isError": false }`. A tool that cannot run (missing scope, bad argument, not found) returns `isError: true` with a message that says what to do next, instead of an HTTP error.

### Tools

`read`: `get_integration_status`, `get_env_template`, `list_products`, `list_provider_accounts`, `list_payment_methods`, `list_payments`, `get_payment`, `list_webhook_deliveries`, `list_inbound_webhook_events`, `get_stats_summary`, `get_docs`, `generate_code`, `wait_for_payment`.

`write`: `create_product`, `update_product`, `rotate_webhook_secret`, `create_api_key`, `revoke_api_key`, `create_test_payment`, `simulate_payment`, `sync_payment`, `retry_webhook_delivery`, `test_webhook_integration`, `run_integration_selftest`.

`get_integration_status` is the entry point: base URL, which provider accounts are enabled per mode, each product with its webhook URL and active keys, and an ordered `next_steps` list ("create a product", "add a webhook URL", "create a sandbox key", ...). `generate_code` returns ready code (`curl`, `typescript`, `python`, `php`) for creating a payment, verifying a webhook and reading a payment, with this deployment's base URL filled in. `test_webhook_integration` creates a mock sandbox payment for a product, marks it paid and reports what the product's webhook endpoint answered (status code, error, attempts).

**Self-testing the application being integrated** (sandbox only, topic `testing`): `run_integration_selftest` delivers signed `payment.paid`, `payment.failed` and `payment.expired` events from mock payments to the product's `webhook_url`, then probes a wrong signature, a missing signature and a stale timestamp (a correct endpoint refuses all three) and the same event twice (it must answer 2xx), and returns a `pass`/`warn`/`fail`/`skipped` checklist with the fix for each; it stops early when nothing answers. `wait_for_payment` (read; waits up to 30 s) returns the payment the running application created through the API with its sandbox key, ignoring `MCP-TEST-` payments, or `found: false` with advice. `simulate_payment` then pays it and, when the product has a webhook URL, delivers the event right away and reports what the application's endpoint answered (`deliver_now: false` skips that).

Secrets: `create_product` and `rotate_webhook_secret` return the webhook secret and `create_api_key` returns the key **once**, exactly like the dashboard. They are shown to the assistant, so the consent screen is where the user decides; the live variants need the `live` scope.

### Resources and prompts

Resources (`resources/list`): `payrouters://docs/<topic>` for `overview`, `authentication`, `create-payment`, `checkout-and-methods`, `payment-status`, `webhooks`, `errors-and-limits`, `sandbox`, `testing`, `providers` (markdown, this deployment's base URL filled in). `get_docs` returns the same text for clients that only support tools.

Prompts: `integrate_payrouters` (arguments `language`, `framework`), `self_test_integration` (argument `product`: tests the running application end to end in sandbox), `debug_payment` (`payment_id` or `order_id`), `go_live_checklist`.

## Storage

Migration `0002_oauth.sql`: `oauth_clients`, `oauth_codes` (hash of the code), `oauth_grants` (client, scopes, current and previous refresh hash, revoked_at) and `oauth_access_tokens` (hash, expiry). Tokens and codes are stored as SHA-256 hashes only. Expired rows are purged hourly.

## Limits

`/oauth/*` and `/.well-known/*` share one per-client limit (`RATE_OAUTH_PER_MIN`, default 60). Failed token exchanges and invalid MCP tokens share one failed-attempt lockout per client (its limit is `RATE_API_KEY_FAILS`; `429` with `Retry-After`, error `slow_down` on the token endpoint). `/mcp` counts towards `RATE_GLOBAL_PER_MIN`.
