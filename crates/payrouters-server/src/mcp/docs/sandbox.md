# Sandbox

- A sandbox key (`prk_test_...`) only reaches sandbox provider accounts: Midtrans sandbox, Xendit test keys and the built-in `mock` provider.
- `mock` needs no credentials. The Snap page shows a simulator that marks the payment paid, expired or failed through the same pipeline as a real provider. Through MCP use `simulate_payment`.
- IDRX has no sandbox: a sandbox key asking for it gets `422 sandbox_unsupported`.
- Sandbox and live data never mix in the dashboard stats.
- A `mock` payment needs a `method` before it can be simulated as paid.
- Fastest proof that an integration works: `run_integration_selftest` (signed paid, failed and expired events plus wrong-signature, missing-signature, stale-timestamp and repeated-event probes against the product's webhook endpoint). `test_webhook_integration` is the single paid event. See topic `testing`.

Going live: add live provider accounts in the dashboard, create a `prk_live_...` key, keep the webhook secret server-side, and re-run the checks in live mode. This MCP connection needs the `live` permission for anything live.
