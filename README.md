# PayRouters

Personal payment router for **cash-in only** (IDR). One service in front of Midtrans, Xendit and IDRX:

- **One webhook per provider, many products.** Each provider account gets a single inbound URL (`/hooks/<account-id>`); PayRouters verifies it, settles the payment and forwards a normalized, signed event to the product that owns it.
- **Standard API, payload, response, SDK** across providers (`docs/api.md`): `POST /v1/payments` is the same whether it ends up at Midtrans, Xendit or IDRX.
- **PayRouters Snap**: a hosted payment page for providers that only expose a core API (virtual account / QRIS instructions, countdown, status polling). Provider-hosted checkout (Midtrans Snap, Xendit Payment Session, IDRX checkout) is available per payment with `checkout: "provider"`.
- **Sandbox mode** like Midtrans: `prk_test_…` keys use sandbox accounts (Midtrans sandbox, Xendit test keys) and a built-in **mock provider** with a simulator (paid / expired / failed) that runs through the exact production pipeline.
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
| `RATE_GLOBAL_PER_MIN` | `600` | `/v1`, `/admin` and `/healthz` per client per minute. The SPA and static files are never limited. |

Limits are per process, in memory (a restart clears them), keyed by client address. Refused requests get `429` with `Retry-After` (seconds) and the usual error envelope, code `rate_limited`. Put `TRUSTED_PROXY_HEADER` only behind a proxy that overwrites the header (Caddy, nginx with `proxy_set_header X-Forwarded-For $remote_addr;`, or Cloudflare for `cf-connecting-ip`); without a proxy, leave it unset, otherwise any client can pick its own address.

Dashboard dev server: `cd web && pnpm dev` (proxies API paths to `127.0.0.1:8080`).

## Setting up providers

Dashboard → Providers → Add account. Then paste the account's **webhook URL** into the provider:

| Provider | Credentials | Where to paste the webhook URL | Verification |
|---|---|---|---|
| Midtrans | Server key (sandbox / production) | Settings → Configuration → Payment Notification URL | `signature_key` (SHA-512) |
| Xendit | Secret API key + webhook verification token | Settings → Webhooks (payment + payment session events) | `x-callback-token` |
| IDRX | API key + secret (live only, no sandbox) + default wallet / chain | Dashboard → API Key → Callback URL (mint) | none (callback is unsigned) → every callback is re-fetched from IDRX Transaction History |
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

- Provider adapters are unit-tested against documented payload shapes; they have **not** been exercised against live Midtrans / Xendit / IDRX endpoints. Run **Test connection** on each account and a sandbox payment before trusting production.
- IDRX documents two different request-signing schemes and has no sandbox. The adapter tries the legacy snippet scheme first and falls back to the newer `METHOD:PATH:SHA256(body):ts` form when IDRX answers 401 (or a 4xx naming the signature), remembering the one that works per account.
- Single admin (static bearer token), no refunds/payouts (by design), only VA and QRIS in Snap core mode (other methods via `checkout: "provider"`).
