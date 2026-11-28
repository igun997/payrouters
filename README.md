# PayRouters

Personal payment router for **cash-in only** (IDR). One service in front of Midtrans, Xendit, IDRX, Mayar and PayPal:

- **One webhook per provider, many products.** Each provider account gets a single inbound URL (`/hooks/<account-id>`); PayRouters verifies it, settles the payment and forwards a normalized, signed event to the product that owns it.
- **Standard API, payload, response, SDK** across providers (`docs/api.md`): `POST /v1/payments` is the same whether it ends up at Midtrans, Xendit, IDRX, Mayar or PayPal.
- **PayRouters Snap**: a hosted payment page for providers that only expose a core API (virtual account / QRIS instructions, countdown, status polling). Provider-hosted checkout (Midtrans Snap, Xendit Payment Session, IDRX checkout, Mayar payment request, PayPal checkout) is available per payment with `checkout: "provider"`; Mayar and PayPal have no standard methods, so they always use it.
- **Sandbox mode** like Midtrans: `prk_test_…` keys use sandbox accounts (Midtrans sandbox, Xendit test keys, Mayar and PayPal sandboxes) and a built-in **mock provider** with a simulator (paid / expired / failed) that runs through the exact production pipeline.
- **Cash-in ledger, stats, recap**: every paid payment writes one ledger row (gross / fee / net), with dashboard stats, per-provider/method/product breakdown, and CSV export.
- Postgres, AES-256-GCM-encrypted provider credentials, transactional outbox with retries for outbound webhooks, reconciliation worker that polls providers (webhooks get lost; IDRX never retries).

```
crates/payrouters-types   wire types shared by server and SDK
crates/payrouters-server  axum + sqlx (Postgres) service, serves the dashboard
crates/payrouters-sdk     Rust SDK + webhook verification
sdk/typescript            @payrouters/sdk (zero-dependency TS SDK)
web/                      SvelteKit SPA (adapter-static): dashboard + public /snap/<token> page
docs/api.md               the API contract (the dashboard also serves a product guide at /docs)
```

## Screenshots

Demo data (mock provider plus fake Midtrans and Xendit accounts). Regenerate with `web/scripts/capture-screenshots.mjs` against a fresh database; its header comment has the commands.

### Dashboard

| | |
|---|---|
| ![Overview: stats, daily chart, share by provider, recent payments](docs/screenshots/overview.png) | ![Payments: status chips, filters and the payment list](docs/screenshots/payments.png) |
| **Overview** | **Payments** |
| ![Payment detail: ledger, activity, instructions and Snap link](docs/screenshots/payment-detail.png) | ![Cash-ins: the gross, fee and net ledger](docs/screenshots/cash-ins.png) |
| **Payment detail** | **Cash-ins** |
| ![Webhooks: inbound provider events with headers and body](docs/screenshots/webhooks.png) | ![Providers: Midtrans, Xendit and the mock simulator](docs/screenshots/providers.png) |
| **Webhooks** | **Providers** |
| ![Integrations: MCP server URL, client snippets and connected apps](docs/screenshots/integrations.png) | ![OAuth consent: an assistant asks to connect, with read, change and live permissions](docs/screenshots/oauth-consent.png) |
| **Integrations (MCP)** | **MCP consent** |

### Phone and the customer page

| | | | |
|---|---|---|---|
| <img src="docs/screenshots/overview-phone.png" alt="Overview on a phone" width="200"> | <img src="docs/screenshots/payments-phone.png" alt="Payments on a phone" width="200"> | <img src="docs/screenshots/snap-choose.png" alt="Snap page: choose a payment method" width="200"> | <img src="docs/screenshots/snap-va.png" alt="Snap page: virtual account instructions" width="200"> |
| **Overview** | **Payments** | **Snap: choose** | **Snap: pay** |

## Run it

```bash
docker compose up -d db                       # Postgres on 127.0.0.1:5435
cargo run -p payrouters-server -- gen-secrets # prints MASTER_KEY and ADMIN_TOKEN
(cd web && pnpm install && pnpm build)        # → web/build

export DATABASE_URL=postgres://payrouters:payrouters@127.0.0.1:5435/payrouters
export PUBLIC_BASE_URL=http://127.0.0.1:8080  # the origin customers and providers reach
export ADMIN_TOKEN=…  MASTER_KEY=…
cargo run -p payrouters-server
```

| Env | Default | Notes |
|---|---|---|
| `DATABASE_URL` | – | Postgres. Migrations run on start. |
| `PUBLIC_BASE_URL` | – | No trailing slash. Used in snap URLs and provider webhook URLs. Must be https in production (IDRX requires an https `returnUrl`). |
| `ADMIN_TOKEN` | – | ≥ 16 chars. Bearer token for the dashboard / `/admin`. |
| `MASTER_KEY` | – | base64, 32 bytes. Encrypts provider credentials and webhook secrets. **Losing it makes stored secrets unrecoverable.** |
| `LISTEN_ADDR` | `0.0.0.0:8080` | |
| `WEB_DIR` | `web/build` | Built SPA. |
| `STATS_TIMEZONE` | `Asia/Jakarta` | Day boundaries for stats and CSV. |
| `LOG_FORMAT` | text | `json` for structured logs. |
| `TRUSTED_PROXY_HEADER` | unset | Header that carries the real client address, e.g. `x-forwarded-for` or `cf-connecting-ip`. Set **only** when a proxy in front of PayRouters overwrites that header on every request (a client must not be able to set it). Unset: the socket peer is the client. |
| `RATE_AUTH_FAILS` | `10` | Failed admin-token attempts per client per window before every request from that client is refused (`429`) until the window ends, including correct tokens. `0` disables. |
| `RATE_API_KEY_FAILS` | `20` | Same, for invalid product API keys on `/v1/*`. |
| `RATE_AUTH_WINDOW_SECS` | `300` | Window for both lockouts above. |
| `RATE_SNAP_PER_MIN` | `120` | Requests per minute per client on `/snap/api/*`. |
| `RATE_SNAP_PAY_PER_MIN` | `20` | Per client per minute on `POST /snap/api/*/pay` and `/simulate`, on top of the snap limit. |
| `RATE_HOOKS_PER_MIN` | `600` | Inbound provider webhooks (`/hooks/*`) per client per minute. Providers share few addresses, so keep this generous. |
| `RATE_GLOBAL_PER_MIN` | `600` | `/v1`, `/admin`, `/mcp` and `/healthz` per client per minute. The SPA and static files are never limited. |
| `RATE_OAUTH_PER_MIN` | `60` | OAuth endpoints and discovery documents (`/oauth/*`, `/.well-known/*`) per client per minute. |

Limits are per process, in memory (a restart clears them), keyed by client address. Refused requests get `429` with `Retry-After` (seconds) and the usual error envelope, code `rate_limited`. Put `TRUSTED_PROXY_HEADER` only behind a proxy that overwrites the header (Caddy, nginx with `proxy_set_header X-Forwarded-For $remote_addr;`, or Cloudflare for `cf-connecting-ip`); without a proxy, leave it unset, otherwise any client can pick its own address.

Dashboard dev server: `cd web && pnpm dev` (proxies API paths to `127.0.0.1:8080`).

## Setting up providers

Dashboard → Providers → Add account. Then paste the account's **webhook URL** into the provider:

| Provider | Credentials | Where to paste the webhook URL | Verification |
|---|---|---|---|
| Midtrans | Server key (sandbox / production) and the **Signature Secret Key** (Settings → Access Keys; since 28 Oct 2026 Midtrans signs notifications with it, not with the server key) | Settings → Configuration → Payment Notification URL (the recurring and account-linking URLs can point at the same hook: they are acknowledged and ignored) | `signature_key` (SHA-512 with the Signature Secret Key; the server key is accepted too) |
| Xendit | Secret API key + webhook verification token | Settings → Webhooks (payment + payment session events) | `x-callback-token` |
| IDRX | API key + secret (live only, no sandbox) + default wallet / chain | Dashboard → API Key → Callback URL (mint) | none (callback is unsigned) → every callback is re-fetched from IDRX Transaction History |
| Mayar | API key (Read & Write; sandbox keys from web.mayar.io) and optionally the **Webhook Token** (Integration → Webhook) | Integration → Webhook (one URL per Mayar account) | the Webhook Token when set (see Known limits), and always: the webhook is not proof, so `payment.received` only names the payment request, whose status is re-read from `GET /hl/v2/payments/{id}` |
| PayPal | Client ID + secret, optional `currency` (default USD), optional fixed `idr_rate` (blank = ECB reference rate) and `webhook_id` | Developer → Apps & Credentials → your app → Webhooks (Checkout order approved, Payment capture events); copy the Webhook ID back into the account | PayPal's `verify-webhook-signature` (needs `webhook_id`), then the order is read again from PayPal |
| Mock | none, auto-created (sandbox) | – | simulator |

Create a product → create a `sandbox` key → `POST /v1/payments`. Sandbox keys only ever touch sandbox accounts; with no real sandbox account the mock provider is used.

## Using it from a product

```ts
import { PayRouters, constructEvent } from '@payrouters/sdk';
const pr = new PayRouters({ apiKey: 'prk_test_…', baseUrl: 'https://pay.example.com' });
const { payment } = await pr.payments.create({ order_id: 'INV-1', amount: 150_000 });
// redirect the customer to payment.snap_url
```

Webhooks to your product carry `PayRouters-Signature: t=…,v1=…` (HMAC-SHA256 of `t.body` with the product's `whsec_…` secret). Delivery is at-least-once with exponential backoff; dedupe on `PayRouters-Event-Id`. A payment that was marked `expired` can still become `paid` later if the provider collected money late: handle `payment.paid` after `payment.expired`.

## Connect an AI assistant (MCP)

PayRouters is also an MCP server with OAuth, so an assistant can set an integration up without anyone pasting tokens or reading the documentation:

```bash
claude mcp add --transport http payrouters https://pay.example.com/mcp
```

The client opens your browser at the consent screen; the signed-in dashboard session approves it (read, make changes, and optionally live mode, which is off by default). Then the assistant can read what is configured (`get_integration_status` lists the next steps), create a product and a sandbox key, generate code with the right base URL, create a test payment and test your running application in sandbox: `run_integration_selftest` checks that your webhook endpoint verifies signatures, refuses forged and stale requests and handles paid, failed, expired and repeated events, and `wait_for_payment` plus `simulate_payment` follow a purchase made in your own app from checkout to paid webhook. Dashboard → **Integrations** has the server URL, a snippet for each client and the list of connected apps with Revoke. Details, scopes and the tool list: `docs/mcp.md`.

## Deploying to a VPS

```bash
rcargo build --release -p payrouters-server --pull   # builds on the remote builder, copies the binary back
```

Copy `target/release/payrouters-server` and `web/build` to the VPS, put the env above in `/etc/payrouters.env`, run it under systemd behind a TLS reverse proxy (Caddy/nginx):

```ini
[Service]
EnvironmentFile=/etc/payrouters.env
ExecStart=/opt/payrouters/payrouters-server
WorkingDirectory=/opt/payrouters
Restart=always
```

Run a single instance (the workers assume it; row locks keep a second one safe but unnecessary).

### Container image and EasyPanel

Pushing a version tag builds the image in GitHub Actions (`.github/workflows/release-image.yml`) and publishes `ghcr.io/igun997/payrouters`:

```bash
# 1. bump `version` under [workspace.package] in Cargo.toml and commit
git tag v0.2.0 && git push origin v0.2.0   # the tag must equal the Cargo version, or the build stops
```

Tags produced: `0.2.0`, `0.2` and, for a release without a `-suffix`, `latest`. The image holds the server and the dashboard (`WEB_DIR=/app/web`), listens on `8080` and answers `/healthz`. It needs a Postgres database and the variables in `.env.example`. Generate the secrets with `docker run --rm ghcr.io/igun997/payrouters:0.2.0 payrouters-server gen-secrets`.

EasyPanel: create an App from the Docker image `ghcr.io/igun997/payrouters:<version>`, add registry credentials when the package is private (a GitHub token with `read:packages`), set the variables from `.env.example` in the Environment tab, point the domain at port `8080`, and add a Postgres service for `DATABASE_URL`. Run one replica.

## Design, UI and end-to-end tests

- Design: Wonder file "PayRouters" (108 artboards, names end with the viewport width). Handoff, breakpoint/type/spacing scale and screen index: `docs/design.md`; generated scale: `docs/responsive.css` (merged into `web/src/routes/layout.css`); builder rules and shared components: `docs/web-conventions.md`.
- Breakpoints: 320 XS, 360 Base, 768 md, 1024 lg, 1280 xl (primary 1366x768), 1536 2xl; type, spacing and layout switch per breakpoint.
- E2E (Playwright, real server + Postgres, built SPA): `docs/e2e.md`. `cd web && pnpm build && pnpm test:e2e` (desktop and phone projects: login, products, providers, payments, cash-ins, webhooks, playground, API docs, live mode, Snap states, a 5-viewport responsive sweep, axe accessibility).

## Tests

```bash
rcargo clippy --workspace --all-targets -- -D warnings
rcargo --pull test --workspace --no-run      # builds on the VPS, copies test binaries back
DATABASE_URL=postgres://payrouters:payrouters@127.0.0.1:5435/payrouters \
  target/debug/build/payrouters-server/<hash>/out/e2e-<hash>   # needs Postgres, which the builder does not have
```

`e2e` drives the real router against a throwaway Postgres per test (mock simulator, signed Midtrans/Xendit webhooks, outbound webhook delivery verified with the SDK, stats/recap/CSV, access control, mode isolation).

## Known limits

- Provider adapters are unit-tested against documented payload shapes; they have **not** been exercised against live Midtrans / Xendit / IDRX / Mayar / PayPal endpoints. Run **Test connection** on each account and a sandbox payment before trusting production.
- IDRX documents two different request-signing schemes and has no sandbox. The adapter tries the legacy snippet scheme first and falls back to the newer `METHOD:PATH:SHA256(body):ts` form when IDRX answers 401 (or a 4xx naming the signature), remembering the one that works per account.
- Provider documentation was re-checked on 2026-11-27 (Midtrans access keys and QRIS, Xendit v3 payment requests and sessions, IDRX mint). Known gaps that are not handled yet:
  - Midtrans: one Signature Secret Key per account (while rotating it, retries signed with the old key are refused until the status poller settles the payment); no `Idempotency-Key` header; no per-method amount bounds; the hosted Snap page shows every enabled method, not only the six PayRouters knows. Core API (`/v2/charge`, used by PayRouters Snap for QRIS and virtual accounts) needs its own activation by Midtrans on a production account; without it every charge answers `402 Payment channel is not activated` although the dashboard lists the channels as active (ask support@midtrans.com). PayRouters detects this: the refused method is remembered for 30 minutes per account, hidden on the Snap page, and the payment goes to Midtrans Snap (hosted) instead, which does not need Core API. Availability is learned from the first refusal (in memory, lost on restart), so the first customer after a restart still makes the failing call and is then served by the fallback.
  - Xendit: session webhooks may not carry `x-callback-token` (the check is kept; the poller settles the payment otherwise); `payment.failure` marks a payment failed even though a session lets the customer retry (a later success still wins); no per-method amount bounds.
  - IDRX: a timeout or 5xx on a direct virtual-account mint is recovered (the order is found again through a `ref:` tag in `productDetails` and its stored payload), but only if the history lists it on its first or last page. A hosted-checkout (`checkout: "provider"`) mint cannot be recovered because IDRX stores no URL to fetch; that unreachable order expires on its own within 24 hours. The documented VA maximum (100.000.000) is below the 1.000.000.000 the adapter accepts.
  - Mayar (documentation checked 2026-11-28, API v2): no virtual account number or QR code is available through the payment request API, so every Mayar payment is the hosted page. Mayar documents no webhook signature; its dashboard issues a Webhook Token "to verify that a webhook comes from Mayar" but Mayar's public documentation does not say where that token travels. When the token is stored on the account, a webhook is accepted only if some request header carries it (whole value or as a bearer token); a refusal names the headers that did arrive, and the token is never written to the Webhooks inbox. Check one real delivery (or Mayar's **Test URL**) after setting it: if Mayar sends the token somewhere else, webhooks are refused (payments still settle by polling within about 5 minutes) and the token should be cleared. Either way a forged webhook costs at most one authenticated read, because status always comes from the API. `redirectUrl` is sent and, if Mayar answers that payment requests do not support it, resent without it (the payer then ends on Mayar's receipt page, and the Snap page, which keeps polling, shows the result). A payment request that Mayar reports as `closed` is stored as `cancelled`; whether an expired link also shows `closed` is not documented. Mayar's fee is not in the API: set a fee schedule on the account. Payments edited in the Mayar dashboard to another amount are refused as an amount mismatch.
  - PayPal (documentation checked 2026-11-28, Orders v2): PayPal does not support IDR. Amounts stay integer rupiah and are converted to the charge currency at the European Central Bank's euro reference rates (public Data API, no key; the rate is the cross of the ECB's IDR and currency rates, cached for an hour, and a payment is refused when the latest ECB rate is more than 7 days old), or at a fixed `idr_rate` set on the account; the customer's total is rounded up to the next minor unit. ECB rates are official but mid-market and published once per working day, so they carry no PayPal conversion spread and lag a moving rupiah by up to a day; set a fixed `idr_rate` when you want a margin built in. The rate and its source are stored with each payment (`provider_data`). The reported PayPal fee is converted back at that rate, and a payer charged in another currency than the merchant holds (PayPal's own conversion) is not reflected. An unapproved order is kept open by PayPal for about 3 hours (longer needs PayPal's account team), whatever `expires_in` says. Only the PayPal wallet flow (`payment_source.paypal`) is used; cards and Pay Later are whatever PayPal offers on its page. Refunds and reversals are not cash-in changes and are ignored. BRL, CNY and MYR are not offered as charge currencies (in-country accounts only).
- Single admin (static bearer token), no refunds/payouts (by design), only VA and QRIS in Snap core mode (other methods via `checkout: "provider"`).
